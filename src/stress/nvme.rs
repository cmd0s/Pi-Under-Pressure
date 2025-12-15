use rand::Rng;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::detection::nvme::NvmeInfo;

/// Test file size (8 GB)
const TEST_FILE_SIZE: u64 = 8 * 1024 * 1024 * 1024;

/// Block size for 4K random I/O
const BLOCK_SIZE_4K: usize = 4096;

/// Block size for sequential I/O
const BLOCK_SIZE_SEQ: usize = 128 * 1024;

/// Run NVMe stress test
pub async fn run_nvme_stress(
    running: Arc<AtomicBool>,
    errors: Arc<AtomicU64>,
    nvme_info: NvmeInfo,
) {
    // Determine test file path - use a temp file on the NVMe
    let test_path = get_test_file_path(&nvme_info);

    // Create test file if needed
    if let Err(e) = create_test_file(&test_path) {
        eprintln!("Failed to create NVMe test file: {}", e);
        errors.fetch_add(1, Ordering::Relaxed);
        return;
    }

    let mut iteration: u64 = 0;

    while running.load(Ordering::Relaxed) {
        // Rotate between different stress methods
        match iteration % 3 {
            0 => {
                if !run_random_4k_stress(&test_path, &running) {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            1 => {
                if !run_sequential_stress(&test_path, &running) {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            2 => {
                if !run_mixed_stress(&test_path, &running) {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            _ => unreachable!(),
        }

        iteration = iteration.wrapping_add(1);
    }

    // Cleanup test file
    let _ = std::fs::remove_file(&test_path);
}

/// Get path for test file on NVMe (public for UI display)
pub fn get_test_file_path(nvme_info: &NvmeInfo) -> PathBuf {
    // Try to find mount point for NVMe
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let device = parts[0];
                let mount_point = parts[1];

                // Check if this is our NVMe device
                if device.contains("nvme") || device.starts_with(&nvme_info.device_path) {
                    // Don't use root filesystem for stress test
                    if mount_point != "/" {
                        let mut path = PathBuf::from(mount_point);
                        path.push(".pi-under-pressure-test");
                        return path;
                    }
                }
            }
        }
    }

    // Fallback to /tmp (which might be on NVMe if root is on NVMe)
    PathBuf::from("/tmp/.pi-under-pressure-nvme-test")
}

/// Create test file filled with random data
fn create_test_file(path: &PathBuf) -> std::io::Result<()> {
    if path.exists() {
        // Check if file is correct size
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.len() == TEST_FILE_SIZE {
                return Ok(());
            }
        }
        std::fs::remove_file(path)?;
    }

    let mut file = File::create(path)?;
    let mut rng = rand::thread_rng();

    // Write in chunks
    let mut buffer = vec![0u8; 1024 * 1024]; // 1 MB buffer
    let mut written = 0u64;

    while written < TEST_FILE_SIZE {
        rng.fill(&mut buffer[..]);
        file.write_all(&buffer)?;
        written += buffer.len() as u64;
    }

    file.sync_all()?;
    Ok(())
}

/// 4K random read/write stress - IOPS test
fn run_random_4k_stress(path: &PathBuf, running: &Arc<AtomicBool>) -> bool {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut rng = rand::thread_rng();
    let mut buffer = vec![0u8; BLOCK_SIZE_4K];
    let max_offset = TEST_FILE_SIZE - BLOCK_SIZE_4K as u64;

    // Do 1000 random I/O operations per iteration
    for _ in 0..1000 {
        if !running.load(Ordering::Relaxed) {
            break;
        }

        let offset = rng.gen_range(0..max_offset);
        let is_read = rng.gen_bool(0.5);

        if let Err(_) = file.seek(SeekFrom::Start(offset)) {
            return false;
        }

        if is_read {
            if let Err(_) = file.read_exact(&mut buffer) {
                return false;
            }
        } else {
            rng.fill(&mut buffer[..]);
            if let Err(_) = file.write_all(&buffer) {
                return false;
            }
        }
    }

    // Sync to ensure writes are committed
    if let Err(_) = file.sync_all() {
        return false;
    }

    true
}

/// Sequential read/write stress - bandwidth test
fn run_sequential_stress(path: &PathBuf, running: &Arc<AtomicBool>) -> bool {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut rng = rand::thread_rng();
    let mut buffer = vec![0u8; BLOCK_SIZE_SEQ];

    // Sequential write
    if let Err(_) = file.seek(SeekFrom::Start(0)) {
        return false;
    }

    let mut written = 0u64;
    while written < TEST_FILE_SIZE && running.load(Ordering::Relaxed) {
        rng.fill(&mut buffer[..]);
        if let Err(_) = file.write_all(&buffer) {
            return false;
        }
        written += buffer.len() as u64;
    }

    if let Err(_) = file.sync_all() {
        return false;
    }

    // Sequential read
    if let Err(_) = file.seek(SeekFrom::Start(0)) {
        return false;
    }

    let mut read = 0u64;
    while read < TEST_FILE_SIZE && running.load(Ordering::Relaxed) {
        if let Err(_) = file.read_exact(&mut buffer) {
            return false;
        }
        read += buffer.len() as u64;
    }

    true
}

/// Mixed workload stress - 70/30 read/write ratio
fn run_mixed_stress(path: &PathBuf, running: &Arc<AtomicBool>) -> bool {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut rng = rand::thread_rng();
    let mut buffer = vec![0u8; BLOCK_SIZE_4K];
    let max_offset = TEST_FILE_SIZE - BLOCK_SIZE_4K as u64;

    // Do 1000 mixed I/O operations
    for _ in 0..1000 {
        if !running.load(Ordering::Relaxed) {
            break;
        }

        let offset = rng.gen_range(0..max_offset);
        let is_read = rng.gen_bool(0.7); // 70% reads

        if let Err(_) = file.seek(SeekFrom::Start(offset)) {
            return false;
        }

        if is_read {
            if let Err(_) = file.read_exact(&mut buffer) {
                return false;
            }
        } else {
            rng.fill(&mut buffer[..]);
            if let Err(_) = file.write_all(&buffer) {
                return false;
            }
        }
    }

    if let Err(_) = file.sync_all() {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn test_get_test_file_path() {
        let nvme = NvmeInfo {
            device_path: "/dev/nvme0n1".to_string(),
            model: "Test".to_string(),
            pcie_gen: Some(3),
        };
        let path = get_test_file_path(&nvme);
        assert!(path.to_string_lossy().contains("pi-under-pressure"));
    }
}
