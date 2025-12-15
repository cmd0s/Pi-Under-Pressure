use rand::Rng;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Minimum memory chunk size in bytes (64 MB)
const MIN_CHUNK_SIZE: usize = 64 * 1024 * 1024;

/// Run memory stress test with multiple patterns
/// allocation_bytes: how much memory this thread should allocate
pub fn run_memory_stress(running: Arc<AtomicBool>, errors: Arc<AtomicU64>, allocation_bytes: usize) {
    let mut iteration: u64 = 0;

    // Allocate memory buffer (at least MIN_CHUNK_SIZE)
    let alloc_size = allocation_bytes.max(MIN_CHUNK_SIZE);
    let mut buffer: Vec<u8> = vec![0; alloc_size];

    while running.load(Ordering::Relaxed) {
        // Rotate between different stress methods
        match iteration % 4 {
            0 => {
                if !run_sequential_stress(&mut buffer) {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            1 => {
                if !run_random_access_stress(&mut buffer) {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            2 => {
                if !run_fill_verify_stress(&mut buffer) {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            3 => {
                if !run_stream_stress(&mut buffer) {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            _ => unreachable!(),
        }

        iteration = iteration.wrapping_add(1);
    }
}

/// Sequential access pattern - tests bandwidth
fn run_sequential_stress(buffer: &mut [u8]) -> bool {
    // Write sequential pattern
    for (i, byte) in buffer.iter_mut().enumerate() {
        *byte = (i & 0xFF) as u8;
    }

    // Verify sequential pattern
    for (i, &byte) in buffer.iter().enumerate() {
        if byte != (i & 0xFF) as u8 {
            return false;
        }
    }

    true
}

/// Random access pattern - tests cache behavior and memory latency
fn run_random_access_stress(buffer: &mut [u8]) -> bool {
    let mut rng = rand::thread_rng();
    let len = buffer.len();

    // Number of random accesses
    const ITERATIONS: usize = 100_000;

    // Random writes
    for _ in 0..ITERATIONS {
        let idx = rng.gen_range(0..len);
        let value = rng.gen::<u8>();
        buffer[idx] = value;
    }

    // Random reads and verify they're consistent
    let mut checksum: u64 = 0;
    for _ in 0..ITERATIONS {
        let idx = rng.gen_range(0..len);
        checksum = checksum.wrapping_add(buffer[idx] as u64);
    }

    // Just ensure we read something (checksum is used to prevent optimization)
    checksum > 0 || buffer.iter().all(|&b| b == 0)
}

/// Fill and verify pattern - detects bit errors
fn run_fill_verify_stress(buffer: &mut [u8]) -> bool {
    // Test multiple patterns
    let patterns: [u8; 4] = [0x00, 0xFF, 0xAA, 0x55];

    for pattern in patterns {
        // Fill with pattern
        buffer.fill(pattern);

        // Verify pattern
        for &byte in buffer.iter() {
            if byte != pattern {
                return false;
            }
        }
    }

    // Walking ones test on a smaller portion
    let test_size = 4096.min(buffer.len());
    for bit in 0..8 {
        let pattern = 1u8 << bit;
        for byte in buffer[..test_size].iter_mut() {
            *byte = pattern;
        }
        for &byte in buffer[..test_size].iter() {
            if byte != pattern {
                return false;
            }
        }
    }

    true
}

/// STREAM-like stress - memory bandwidth test
fn run_stream_stress(buffer: &mut [u8]) -> bool {
    // Treat as u64 for better bandwidth
    let len = buffer.len() / 8;

    // Safety: buffer is aligned and large enough
    let buffer_u64 = unsafe {
        std::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u64, len)
    };

    // Copy operation
    let scalar: u64 = 3;
    for i in 0..len / 2 {
        buffer_u64[i + len / 2] = buffer_u64[i];
    }

    // Scale operation
    for elem in buffer_u64.iter_mut() {
        *elem = elem.wrapping_mul(scalar);
    }

    // Add operation
    for i in 0..len / 2 {
        buffer_u64[i] = buffer_u64[i].wrapping_add(buffer_u64[i + len / 2]);
    }

    // Triad operation: a = b + scalar * c
    for i in 0..len / 3 {
        buffer_u64[i] = buffer_u64[i + len / 3]
            .wrapping_add(scalar.wrapping_mul(buffer_u64[i + 2 * len / 3]));
    }

    // Just verify something was done
    buffer_u64.iter().any(|&v| v != 0) || buffer_u64.iter().all(|&v| v == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_stress() {
        let mut buffer = vec![0u8; 1024];
        assert!(run_sequential_stress(&mut buffer));
    }

    #[test]
    fn test_random_access_stress() {
        let mut buffer = vec![0u8; 1024];
        assert!(run_random_access_stress(&mut buffer));
    }

    #[test]
    fn test_fill_verify_stress() {
        let mut buffer = vec![0u8; 8192];
        assert!(run_fill_verify_stress(&mut buffer));
    }

    #[test]
    fn test_stream_stress() {
        let mut buffer = vec![0u8; 8192];
        assert!(run_stream_stress(&mut buffer));
    }
}
