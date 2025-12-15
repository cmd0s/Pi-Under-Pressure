use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Run video encoder stress test using hardware acceleration
/// This requires ffmpeg with v4l2 support and a test video
pub fn run_video_stress(running: Arc<AtomicBool>, errors: Arc<AtomicU64>) {
    // Check if ffmpeg is available
    if !is_ffmpeg_available() {
        eprintln!("Warning: ffmpeg not found, skipping video stress test");
        return;
    }

    // Check if V4L2 encoder is available
    if !is_v4l2_encoder_available() {
        eprintln!("Warning: V4L2 H.265 encoder not available, skipping video stress test");
        return;
    }

    // Create test input if needed
    let test_input = "/tmp/.pi-under-pressure-video-input.yuv";
    if let Err(e) = create_test_video(test_input) {
        eprintln!("Warning: Failed to create test video: {}", e);
        return;
    }

    while running.load(Ordering::Relaxed) {
        if !run_encode_cycle(test_input) {
            errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(test_input);
    let _ = std::fs::remove_file("/tmp/.pi-under-pressure-video-output.h265");
}

/// Check if ffmpeg is available
fn is_ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if V4L2 H.265 encoder is available
fn is_v4l2_encoder_available() -> bool {
    // Check for V4L2 M2M device
    if std::path::Path::new("/dev/video10").exists() {
        return true;
    }

    // Check ffmpeg encoders
    if let Ok(output) = Command::new("ffmpeg").args(["-encoders"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return stdout.contains("hevc_v4l2m2m") || stdout.contains("h264_v4l2m2m");
    }

    false
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
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to generate test video",
        ))
    }
}

/// Run a single encode cycle using hardware encoder
fn run_encode_cycle(input_path: &str) -> bool {
    let output_path = "/tmp/.pi-under-pressure-video-output.h265";

    // Try V4L2 H.265 encoder first, fallback to H.264
    let encoder = if check_encoder_available("hevc_v4l2m2m") {
        "hevc_v4l2m2m"
    } else if check_encoder_available("h264_v4l2m2m") {
        "h264_v4l2m2m"
    } else {
        // Fallback to software encoder (still stresses CPU)
        "libx265"
    };

    let result = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "yuv420p",
            "-s",
            "1280x720",
            "-r",
            "30",
            "-i",
            input_path,
            "-c:v",
            encoder,
            "-f",
            "null",
            "-", // Discard output
        ])
        .output();

    match result {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Check if a specific encoder is available
fn check_encoder_available(encoder: &str) -> bool {
    if let Ok(output) = Command::new("ffmpeg").args(["-encoders"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return stdout.contains(encoder);
    }
    false
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
    fn test_v4l2_check() {
        // This test just checks the function doesn't panic
        let _ = is_v4l2_encoder_available();
    }
}
