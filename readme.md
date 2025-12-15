# Pi Under Pressure

**Stability tester for overclocked Raspberry Pi 5**

Pi Under Pressure is a comprehensive stress testing tool designed specifically for Raspberry Pi 5 after overclocking. It tests CPU, RAM, and NVMe stability simultaneously while monitoring temperature, throttling, and system errors.

## Features

- **CPU Stress Testing**: FFT, matrix multiplication, prime sieve, and AES-256 workloads
- **Memory Stress Testing**: Random access, sequential patterns, and STREAM-like bandwidth tests
- **NVMe Stress Testing**: 4K random I/O and sequential bandwidth tests (auto-detected)
- **Video Encoder Stress**: Optional hardware H.265 encoder stress via V4L2 (`--video`)
- **Real-time Monitoring**: Temperature, frequency, throttling status, and errors
- **Fancy TUI**: Interactive terminal UI with progress bars and gauges
- **Error Detection**: Monitors dmesg/journalctl for I/O errors and kernel issues
- **Comprehensive Reports**: Final stability report with pass/fail status

## Installation

### One-liner Install

```bash
curl -sSL https://raw.githubusercontent.com/cmd0s/Pi-Under-Pressure/main/install.sh | bash
```

### Manual Download

Download the latest binary from [Releases](https://github.com/cmd0s/Pi-Under-Pressure/releases):

```bash
# Download
wget https://github.com/cmd0s/Pi-Under-Pressure/releases/latest/download/pi-under-pressure-linux-arm64

# Make executable
chmod +x pi-under-pressure-linux-arm64

# Move to PATH
sudo mv pi-under-pressure-linux-arm64 /usr/local/bin/pi-under-pressure
```

### Build from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/cmd0s/Pi-Under-Pressure.git
cd Pi-Under-Pressure
cargo build --release

# Binary is at target/release/pi-under-pressure
```

## Usage

### Basic Usage

**Note:** Root privileges (sudo) are required for full functionality.

```bash
# Run 30-minute stability test (default)
sudo pi-under-pressure

# Run 1-hour test
sudo pi-under-pressure --duration 1h

# Run 2-hour test with video encoder stress
sudo pi-under-pressure --duration 2h --video
```

### Options

```
OPTIONS:
    -d, --duration <TIME>     Test duration (e.g., 30m, 1h, 2h30m) [default: 30m]
    -e, --extended            Force extended mode (include NVMe stress)
    --video                   Enable hardware video encoder stress
    -c, --cpu-only            Test only CPU (skip RAM and NVMe)
    -m, --memory-only         Test only RAM
    -n, --nvme-only           Test only NVMe
    -t, --threads <N>         Number of CPU threads [default: all cores]
    -i, --interval <SEC>      Status update interval [default: 2]
    --simple                  Use simple output instead of TUI
    --no-color                Disable colors
    --json                    Output final report in JSON format
    -h, --help                Print help
    -V, --version             Print version

CONTROLS:
    Ctrl+C or 'q'             Stop test gracefully
```

## System Information Display

On startup, Pi Under Pressure displays comprehensive system information:

```
+----------------------------------------------------------------+
|  Pi Under Pressure v0.1.0 - Stability Tester for RPi5          |
+----------------------------------------------------------------+
|  SYSTEM                                                        |
|  Model:          Raspberry Pi 5 Model B Rev 1.0                |
|  Serial:         10000000xxxxxxxx                              |
|  Firmware:       Dec  6 2024 14:32:sp (xxxxx)                  |
|  CPU:            ARM Cortex-A76 (4 cores)                      |
|  RAM:            8192 MB                                       |
|  OS:             Debian GNU/Linux 12 (bookworm) aarch64        |
|  Kernel:         6.6.31+rpt-rpi-2712                           |
+----------------------------------------------------------------+
|  OVERCLOCKING (/boot/firmware/config.txt)                      |
|  arm_freq:           2800 MHz (default: 2400)                  |
|  gpu_freq:           1000 MHz (default: 910)                   |
|  over_voltage_delta: 50000 uV (+50.0mV)                        |
|  force_turbo:        1                                         |
+----------------------------------------------------------------+
|  STORAGE                                                       |
|  NVMe Detected:      Samsung 980 PRO 500GB                     |
|  PCIe Generation:    Gen 3.0 x1 (~900 MB/s)                    |
|  NVMe Temperature:   35C                                       |
+----------------------------------------------------------------+
```

## Stability Test Report

After the test completes, a detailed report is generated:

```
===================================================================
                    STABILITY TEST RESULTS
===================================================================
Duration:          30:00
Result:            PASSED (system is stable)

Workloads:
  CPU Stress:        [OK] No computation errors
  Memory Stress:     [OK] All patterns verified
  NVMe Stress:       [OK] No I/O errors

Temperature Stats:
  CPU Max:         82.0C (threshold: 85C)
  CPU Avg:         76.5C
  NVMe Max:        52.0C

Events:
  Throttling:      0
  Under-voltage:   0
  I/O Errors:      0
  SMART Warnings:  0
===================================================================
```

## Recommended Overclocking Settings

### Conservative (Passive Cooling)

```ini
# /boot/firmware/config.txt
arm_freq=2600
gpu_freq=900
over_voltage_delta=25000
```

### Moderate (Active Cooling Required)

```ini
arm_freq=2800
gpu_freq=1000
over_voltage_delta=50000
```

### Aggressive (High-RPM Fan + Good Silicon)

```ini
arm_freq=3000
gpu_freq=1100
over_voltage_delta=75000
force_turbo=1
```

## Requirements

- Raspberry Pi 5 (other models may work but are not officially supported)
- Raspberry Pi OS (Bookworm) or compatible Linux distribution
- **Root privileges (sudo)** - required for accessing hardware sensors and NVMe stress testing
- `vcgencmd` for temperature/throttling monitoring (usually pre-installed)
- Optional: `smartctl` for NVMe SMART data
- Optional: `ffmpeg` for video encoder stress testing

**Note:** Run with `sudo` for full functionality:
```bash
sudo pi-under-pressure --duration 30m
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## Acknowledgments

- Raspberry Pi Foundation for the excellent hardware
- The Rust community for amazing tools and libraries
- stress-ng and fio for inspiration on stress testing approaches
