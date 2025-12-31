use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes256;
use rand::Rng;

/// Run CPU stress test with multiple workloads
pub fn run_cpu_stress(running: Arc<AtomicBool>, errors: Arc<AtomicU64>) {
    let mut iteration: u64 = 0;

    while running.load(Ordering::Relaxed) {
        // Rotate between different stress methods
        match iteration % 4 {
            0 => {
                if !run_dft_stress() {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            1 => {
                if !run_matrix_stress() {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            2 => {
                if !run_prime_stress() {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            3 => {
                if !run_aes_stress() {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            _ => unreachable!(),
        }

        iteration = iteration.wrapping_add(1);
    }
}

/// DFT stress test - floating-point intensive
fn run_dft_stress() -> bool {
    const SIZE: usize = 4096;

    // Create test data
    let mut real: Vec<f64> = (0..SIZE).map(|i| (i as f64).sin()).collect();
    let mut imag: Vec<f64> = vec![0.0; SIZE];

    // Simple DFT (not optimized, but CPU-intensive)
    let mut result_real = vec![0.0; SIZE];
    let mut result_imag = vec![0.0; SIZE];

    for k in 0..SIZE {
        let mut sum_real = 0.0;
        let mut sum_imag = 0.0;

        for n in 0..SIZE {
            let angle = -2.0 * std::f64::consts::PI * (k as f64) * (n as f64) / (SIZE as f64);
            sum_real += real[n] * angle.cos() - imag[n] * angle.sin();
            sum_imag += real[n] * angle.sin() + imag[n] * angle.cos();
        }

        result_real[k] = sum_real;
        result_imag[k] = sum_imag;
    }

    // Inverse DFT to verify
    for k in 0..SIZE {
        let mut sum_real = 0.0;
        let mut sum_imag = 0.0;

        for n in 0..SIZE {
            let angle = 2.0 * std::f64::consts::PI * (k as f64) * (n as f64) / (SIZE as f64);
            sum_real += result_real[n] * angle.cos() - result_imag[n] * angle.sin();
            sum_imag += result_real[n] * angle.sin() + result_imag[n] * angle.cos();
        }

        real[k] = sum_real / SIZE as f64;
        imag[k] = sum_imag / SIZE as f64;
    }

    // Verify result (original signal should be approximately recovered)
    let expected_first = 0.0_f64.sin();
    let tolerance = 0.001;

    (real[0] - expected_first).abs() < tolerance
}

/// Matrix multiplication stress - SIMD friendly
fn run_matrix_stress() -> bool {
    const SIZE: usize = 128;

    // Create matrices
    let mut a = vec![vec![0.0f64; SIZE]; SIZE];
    let mut b = vec![vec![0.0f64; SIZE]; SIZE];
    let mut c = vec![vec![0.0f64; SIZE]; SIZE];

    // Initialize with deterministic values
    for i in 0..SIZE {
        for j in 0..SIZE {
            a[i][j] = ((i * SIZE + j) % 100) as f64 / 100.0;
            b[i][j] = ((j * SIZE + i) % 100) as f64 / 100.0;
        }
    }

    // Matrix multiplication
    for i in 0..SIZE {
        for j in 0..SIZE {
            let mut sum = 0.0;
            for k in 0..SIZE {
                sum += a[i][k] * b[k][j];
            }
            c[i][j] = sum;
        }
    }

    // Verify a known element
    // c[0][0] = sum of a[0][k] * b[k][0] for k=0..SIZE
    let mut expected = 0.0;
    for k in 0..SIZE {
        expected += a[0][k] * b[k][0];
    }

    (c[0][0] - expected).abs() < 0.0001
}

/// Prime sieve stress - integer operations
fn run_prime_stress() -> bool {
    const LIMIT: usize = 100_000;

    // Sieve of Eratosthenes
    let mut is_prime = vec![true; LIMIT + 1];
    is_prime[0] = false;
    is_prime[1] = false;

    let mut i = 2;
    while i * i <= LIMIT {
        if is_prime[i] {
            let mut j = i * i;
            while j <= LIMIT {
                is_prime[j] = false;
                j += i;
            }
        }
        i += 1;
    }

    // Count primes
    let prime_count = is_prime.iter().filter(|&&p| p).count();

    // Known value: there are 9592 primes below 100000
    prime_count == 9592
}

/// AES stress test - uses hardware crypto if available
fn run_aes_stress() -> bool {
    // 256-bit key
    let key: [u8; 32] = [
        0x60, 0x3d, 0xeb, 0x10, 0x15, 0xca, 0x71, 0xbe, 0x2b, 0x73, 0xae, 0xf0, 0x85, 0x7d, 0x77,
        0x81, 0x1f, 0x35, 0x2c, 0x07, 0x3b, 0x61, 0x08, 0xd7, 0x2d, 0x98, 0x10, 0xa3, 0x09, 0x14,
        0xdf, 0xf4,
    ];

    let cipher = Aes256::new_from_slice(&key).unwrap();

    // Encrypt many blocks
    let mut rng = rand::thread_rng();
    let mut block = [0u8; 16];
    rng.fill(&mut block);

    let original_block = block;

    // Encrypt many times
    for _ in 0..10000 {
        cipher.encrypt_block((&mut block).into());
    }

    // The block should have changed
    block != original_block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dft_stress() {
        assert!(run_dft_stress());
    }

    #[test]
    fn test_matrix_stress() {
        assert!(run_matrix_stress());
    }

    #[test]
    fn test_prime_stress() {
        assert!(run_prime_stress());
    }

    #[test]
    fn test_aes_stress() {
        assert!(run_aes_stress());
    }
}
