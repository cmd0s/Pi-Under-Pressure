pub mod cpu;
pub mod memory;
pub mod nvme;
pub mod video;

use crate::detection::{self, nvme::NvmeInfo};
use crate::system::monitor::{self, CpuStatSnapshot, FanStatus, MonitorStats, ThrottleStatus};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct StressConfig {
    pub cpu: bool,
    pub memory: bool,
    pub nvme: bool,
    pub video: bool,
    pub threads: usize,
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StressStats {
    pub elapsed_secs: u64,
    pub cpu_temp_c: f32,
    pub cpu_temp_max: f32,
    pub cpu_freq_mhz: u32,
    pub throttle_status: ThrottleStatus,
    pub cpu_usage_per_core: Vec<f32>,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub nvme_temp_c: Option<f32>,
    pub nvme_temp_max: Option<f32>,
    pub nvme_test_path: Option<String>,
    pub io_errors: u32,
    pub cpu_errors: u64,
    pub memory_errors: u64,
    pub nvme_errors: u64,
    pub progress_percent: f32,
    pub fan_status: FanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub cpu_errors: u64,
    pub memory_errors: u64,
    pub nvme_errors: u64,
    pub video_errors: u64,
    pub throttle_events: u32,
    pub under_voltage_events: u32,
    pub max_cpu_temp: f32,
    pub avg_cpu_temp: f32,
    pub max_nvme_temp: Option<f32>,
    pub completed: bool,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalReport {
    pub passed: bool,
    pub duration_secs: u64,
    pub cpu_stress_passed: bool,
    pub memory_stress_passed: bool,
    pub nvme_stress_passed: bool,
    pub video_stress_passed: bool,
    pub max_cpu_temp: f32,
    pub avg_cpu_temp: f32,
    pub max_nvme_temp: Option<f32>,
    pub throttle_events: u32,
    pub under_voltage_events: u32,
    pub io_errors: u32,
    pub smart_warnings: u32,
}

pub async fn run_stress_test(
    config: StressConfig,
    running: Arc<AtomicBool>,
    stats_tx: mpsc::Sender<StressStats>,
    _event_tx: mpsc::Sender<String>,
    nvme_info: Option<NvmeInfo>,
) -> TestResult {
    let start_time = Instant::now();
    let cpu_errors = Arc::new(AtomicU64::new(0));
    let memory_errors = Arc::new(AtomicU64::new(0));
    let nvme_errors = Arc::new(AtomicU64::new(0));
    let video_errors = Arc::new(AtomicU64::new(0));

    let mut temp_samples: Vec<f32> = Vec::new();
    let mut max_cpu_temp: f32 = 0.0;
    let mut max_nvme_temp: Option<f32> = None;
    let mut throttle_events: u32 = 0;
    let mut under_voltage_events: u32 = 0;

    // Start CPU stress threads
    let cpu_handles: Vec<_> = if config.cpu {
        (0..config.threads)
            .map(|_| {
                let running = running.clone();
                let errors = cpu_errors.clone();
                std::thread::spawn(move || {
                    cpu::run_cpu_stress(running, errors);
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    // Start memory stress threads
    // Calculate memory allocation: use half of total RAM, divided among threads
    let mem_handles: Vec<_> = if config.memory {
        let (_, total_mb) = monitor::get_memory_usage();
        let num_mem_threads = 2.min(config.threads);
        // Use half of total RAM, divided by number of threads
        let allocation_per_thread = (total_mb as usize * 1024 * 1024 / 2) / num_mem_threads;

        (0..num_mem_threads)
            .map(|_| {
                let running = running.clone();
                let errors = memory_errors.clone();
                std::thread::spawn(move || {
                    memory::run_memory_stress(running, errors, allocation_per_thread);
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    // Start NVMe stress if enabled and available
    let nvme_test_path = if config.nvme && nvme_info.is_some() {
        Some(nvme::get_test_file_path(nvme_info.as_ref().unwrap()).to_string_lossy().to_string())
    } else {
        None
    };

    let nvme_handle = if config.nvme && nvme_info.is_some() {
        let running = running.clone();
        let errors = nvme_errors.clone();
        let nvme = nvme_info.clone().unwrap();
        Some(tokio::spawn(async move {
            nvme::run_nvme_stress(running, errors, nvme).await;
        }))
    } else {
        None
    };

    // Start video stress if enabled
    let video_handle = if config.video {
        let running = running.clone();
        let errors = video_errors.clone();
        Some(std::thread::spawn(move || {
            video::run_video_stress(running, errors);
        }))
    } else {
        None
    };

    // Monitoring loop
    let mut last_throttle_raw: u32 = 0;
    let mut cpu_snapshot = CpuStatSnapshot::read();

    while running.load(Ordering::SeqCst) {
        let elapsed = start_time.elapsed();
        if elapsed >= config.duration {
            running.store(false, Ordering::SeqCst);
            break;
        }

        // Collect stats with CPU usage
        let (monitor_stats, new_snapshot) = monitor::collect_stats_with_cpu(&cpu_snapshot);
        cpu_snapshot = new_snapshot;

        // Track temperature
        temp_samples.push(monitor_stats.cpu_temp_c);
        if monitor_stats.cpu_temp_c > max_cpu_temp {
            max_cpu_temp = monitor_stats.cpu_temp_c;
        }

        // Track NVMe temperature
        if let Some(ref nvme) = nvme_info {
            if let Some(temp) = detection::nvme::get_nvme_temp(&nvme.device_path) {
                match max_nvme_temp {
                    Some(max) if temp > max => max_nvme_temp = Some(temp),
                    None => max_nvme_temp = Some(temp),
                    _ => {}
                }
            }
        }

        // Track throttle events (count changes from 0 to non-zero)
        let current_throttle = monitor_stats.throttle_status.raw_value;
        if current_throttle != last_throttle_raw {
            if monitor_stats.throttle_status.throttled_now
                || monitor_stats.throttle_status.freq_capped_now
                || monitor_stats.throttle_status.soft_temp_limit_now
            {
                throttle_events += 1;
            }
            if monitor_stats.throttle_status.under_voltage_now {
                under_voltage_events += 1;
            }
        }
        last_throttle_raw = current_throttle;

        // Build stats
        let stats = StressStats {
            elapsed_secs: elapsed.as_secs(),
            cpu_temp_c: monitor_stats.cpu_temp_c,
            cpu_temp_max: max_cpu_temp,
            cpu_freq_mhz: monitor_stats.cpu_freq_mhz,
            throttle_status: monitor_stats.throttle_status,
            cpu_usage_per_core: monitor_stats.cpu_usage_per_core,
            mem_used_mb: monitor_stats.mem_used_mb,
            mem_total_mb: monitor_stats.mem_total_mb,
            nvme_temp_c: if nvme_info.is_some() {
                detection::nvme::get_nvme_temp(
                    &nvme_info.as_ref().unwrap().device_path,
                )
            } else {
                None
            },
            nvme_temp_max: max_nvme_temp,
            nvme_test_path: nvme_test_path.clone(),
            io_errors: detection::errors::count_recent_io_errors(),
            cpu_errors: cpu_errors.load(Ordering::Relaxed),
            memory_errors: memory_errors.load(Ordering::Relaxed),
            nvme_errors: nvme_errors.load(Ordering::Relaxed),
            progress_percent: (elapsed.as_secs_f32() / config.duration.as_secs_f32()) * 100.0,
            fan_status: monitor_stats.fan_status,
        };

        // Send stats (ignore errors if receiver dropped)
        let _ = stats_tx.send(stats).await;

        // Sleep before next update
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // Signal stop
    running.store(false, Ordering::SeqCst);

    // Wait for threads to finish
    for handle in cpu_handles {
        let _ = handle.join();
    }
    for handle in mem_handles {
        let _ = handle.join();
    }
    if let Some(handle) = nvme_handle {
        let _ = handle.await;
    }
    if let Some(handle) = video_handle {
        let _ = handle.join();
    }

    // Calculate average temperature
    let avg_cpu_temp = if temp_samples.is_empty() {
        0.0
    } else {
        temp_samples.iter().sum::<f32>() / temp_samples.len() as f32
    };

    TestResult {
        cpu_errors: cpu_errors.load(Ordering::Relaxed),
        memory_errors: memory_errors.load(Ordering::Relaxed),
        nvme_errors: nvme_errors.load(Ordering::Relaxed),
        video_errors: video_errors.load(Ordering::Relaxed),
        throttle_events,
        under_voltage_events,
        max_cpu_temp,
        avg_cpu_temp,
        max_nvme_temp,
        completed: start_time.elapsed() >= config.duration,
        duration_secs: start_time.elapsed().as_secs(),
    }
}

pub fn generate_report(
    result: &TestResult,
    io_errors: &[String],
    _duration: Duration,
) -> FinalReport {
    let cpu_passed = result.cpu_errors == 0;
    let memory_passed = result.memory_errors == 0;
    let nvme_passed = result.nvme_errors == 0 && io_errors.is_empty();
    let video_passed = result.video_errors == 0;

    let passed = cpu_passed
        && memory_passed
        && nvme_passed
        && video_passed
        && result.throttle_events == 0
        && result.under_voltage_events == 0;

    FinalReport {
        passed,
        duration_secs: result.duration_secs,
        cpu_stress_passed: cpu_passed,
        memory_stress_passed: memory_passed,
        nvme_stress_passed: nvme_passed,
        video_stress_passed: video_passed,
        max_cpu_temp: result.max_cpu_temp,
        avg_cpu_temp: result.avg_cpu_temp,
        max_nvme_temp: result.max_nvme_temp,
        throttle_events: result.throttle_events,
        under_voltage_events: result.under_voltage_events,
        io_errors: io_errors.len() as u32,
        smart_warnings: 0, // TODO: implement SMART checking
    }
}
