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
    custom_path: Option<String>,
) {
    // Determine test file path - use a temp file on the NVMe
    let test_path = get_test_file_path(&nvme_info, custom_path.as_deref());

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
/// custom_path: User-specified path via --nvme-path flag
pub fn get_test_file_path(_nvme_info: &NvmeInfo, custom_path: Option<&str>) -> PathBuf {
    // 1. If user specified custom path, use it
    if let Some(path) = custom_path {
        return PathBuf::from(path);
    }

    // 2. Check if root "/" is on NVMe - if so, use user's home or /var/tmp
    //    (NOT /tmp which is often tmpfs in RAM!)
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let device = parts[0];
                let mount_point = parts[1];

                // Root is on NVMe? Use home dir or /var/tmp (both on NVMe)
                if mount_point == "/" && device.contains("nvme") {
                    // Try user's cache directory first
                    if let Some(home) = std::env::var_os("HOME") {
                        let mut cache_path = PathBuf::from(home);
                        cache_path.push(".cache");
                        cache_path.push("pi-under-pressure");
                        // Create cache dir if needed
                        let _ = std::fs::create_dir_all(&cache_path);
                        cache_path.push("nvme-test");
                        return cache_path;
                    }
                    // Fallback to /var/tmp (persistent, on root fs)
                    return PathBuf::from("/var/tmp/.pi-under-pressure-nvme-test");
                }
            }
        }

        // 3. Look for non-boot NVMe mount with enough space
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let device = parts[0];
                let mount_point = parts[1];

                if device.contains("nvme") {
                    // Skip root and boot partitions
                    if mount_point == "/"
                        || mount_point == "/boot"
                        || mount_point == "/boot/firmware"
                    {
                        continue;
                    }
                    // Found a separate NVMe data partition
                    let mut path = PathBuf::from(mount_point);
                    path.push(".pi-under-pressure-test");
                    return path;
                }
            }
        }
    }

    // 4. Fallback to /var/tmp (NOT /tmp which may be tmpfs)
    PathBuf::from("/var/tmp/.pi-under-pressure-nvme-test")
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

        if file.seek(SeekFrom::Start(offset)).is_err() {
            return false;
        }

        if is_read {
            if file.read_exact(&mut buffer).is_err() {
                return false;
            }
        } else {
            rng.fill(&mut buffer[..]);
            if file.write_all(&buffer).is_err() {
                return false;
            }
        }
    }

    // Sync to ensure writes are committed
    if file.sync_all().is_err() {
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
    if file.seek(SeekFrom::Start(0)).is_err() {
        return false;
    }

    let mut written = 0u64;
    while written < TEST_FILE_SIZE && running.load(Ordering::Relaxed) {
        rng.fill(&mut buffer[..]);
        if file.write_all(&buffer).is_err() {
            return false;
        }
        written += buffer.len() as u64;
    }

    if file.sync_all().is_err() {
        return false;
    }

    // Sequential read
    if file.seek(SeekFrom::Start(0)).is_err() {
        return false;
    }

    let mut read = 0u64;
    while read < TEST_FILE_SIZE && running.load(Ordering::Relaxed) {
        if file.read_exact(&mut buffer).is_err() {
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

        if file.seek(SeekFrom::Start(offset)).is_err() {
            return false;
        }

        if is_read {
            if file.read_exact(&mut buffer).is_err() {
                return false;
            }
        } else {
            rng.fill(&mut buffer[..]);
            if file.write_all(&buffer).is_err() {
                return false;
            }
        }
    }

    if file.sync_all().is_err() {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_test_file_path() {
        let nvme = NvmeInfo {
            device_path: "/dev/nvme0n1".to_string(),
            model: "Test".to_string(),
            pcie_gen: Some(3),
        };
        // Test with auto-detection (None)
        let path = get_test_file_path(&nvme, None);
        assert!(path.to_string_lossy().contains("pi-under-pressure"));

        // Test with custom path
        let custom_path = get_test_file_path(&nvme, Some("/custom/path/test"));
        assert_eq!(custom_path.to_string_lossy(), "/custom/path/test");
    }
}
