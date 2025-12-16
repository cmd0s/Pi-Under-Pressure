use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Pre-detect working encoder before starting stress test
/// Call this BEFORE TUI starts to avoid terminal corruption from V4L2 driver
pub fn detect_encoder() -> Option<&'static str> {
    if !is_ffmpeg_available() {
        return None;
    }
    find_working_encoder()
}

/// Run video encoder stress test with pre-detected encoder
/// Use detect_encoder() before TUI starts, then pass result here
pub fn run_video_stress_with_encoder(
    running: Arc<AtomicBool>,
    errors: Arc<AtomicU64>,
    encoder: &'static str,
) {
    // Create test input if needed
    let test_input = "/tmp/.pi-under-pressure-video-input.yuv";
    if let Err(e) = create_test_video(test_input) {
        eprintln!("Warning: Failed to create test video: {}", e);
        return;
    }

    while running.load(Ordering::Relaxed) {
        if !run_encode_cycle(test_input, encoder) {
            errors.fetch_add(1, Ordering::Relaxed);
        }

        // Small delay between cycles to prevent overwhelming the system
        thread::sleep(Duration::from_millis(100));
    }

    // Cleanup
    let _ = std::fs::remove_file(test_input);
    let _ = std::fs::remove_file("/tmp/.pi-under-pressure-video-output.h265");
}

/// Check if ffmpeg is available
fn is_ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Create a test video input (raw YUV data)
fn create_test_video(path: &str) -> std::io::Result<()> {
    // Generate 100 frames of 720p YUV420 test pattern
    // Using ffmpeg to generate test pattern
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=5:size=1280x720:rate=30",
            "-pix_fmt",
            "yuv420p",
            "-f",
            "rawvideo",
            path,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("Failed to generate test video"))
    }
}

/// Run a single encode cycle using the specified encoder
fn run_encode_cycle(input_path: &str, encoder: &str) -> bool {
    let result = Command::new("ffmpeg")
        .args([
            "-y", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-s", "1280x720", "-r", "30", "-i",
            input_path, "-c:v", encoder, "-f", "null", "-", // Discard output
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    result.map(|s| s.success()).unwrap_or(false)
}

/// Check if a specific encoder is available (just checks listing)
fn check_encoder_available(encoder: &str) -> bool {
    let result = Command::new("ffmpeg")
        .args(["-encoders"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();

    if let Ok(output) = result {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return stdout.contains(encoder);
    }
    false
}

/// Actually test if encoder works by encoding a single frame
/// This catches cases where encoder is listed but hardware isn't available
fn test_encoder_works(encoder: &str) -> bool {
    let result = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=320x240:rate=10",
            "-frames:v",
            "1",
            "-c:v",
            encoder,
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    result.map(|s| s.success()).unwrap_or(false)
}

/// Find a working encoder, testing each one
fn find_working_encoder() -> Option<&'static str> {
    // Try hardware encoders first
    for encoder in &["hevc_v4l2m2m", "h264_v4l2m2m"] {
        if check_encoder_available(encoder) && test_encoder_works(encoder) {
            return Some(*encoder);
        }
    }

    // Fallback to software encoder (libx264 is lighter than libx265)
    if check_encoder_available("libx264") && test_encoder_works("libx264") {
        return Some("libx264");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffmpeg_check() {
        // This test just checks the function doesn't panic
        let _ = is_ffmpeg_available();
    }

    #[test]
    fn test_find_encoder() {
        // This test just checks the function doesn't panic
        let _ = find_working_encoder();
    }
}
