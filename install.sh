#!/bin/bash
#
# Pi Under Pressure - Installer Script
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/cmd0s/Pi-Under-Pressure/main/install.sh | bash
#
# Or with custom install path:
#   curl -sSL https://raw.githubusercontent.com/cmd0s/Pi-Under-Pressure/main/install.sh | bash -s -- /custom/path
#

set -e

# Configuration
REPO="cmd0s/Pi-Under-Pressure"
BINARY_NAME="pi-under-pressure"
DEFAULT_INSTALL_DIR="/usr/local/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Functions
info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Check architecture
ARCH=$(uname -m)
case $ARCH in
    aarch64|arm64)
        ASSET_NAME="${BINARY_NAME}-linux-arm64"
        ;;
    x86_64)
        ASSET_NAME="${BINARY_NAME}-linux-amd64"
        ;;
    *)
        error "Unsupported architecture: $ARCH"
        ;;
esac

# Determine install directory
INSTALL_DIR="${1:-$DEFAULT_INSTALL_DIR}"

# Check if we need sudo
NEED_SUDO=""
if [ ! -w "$INSTALL_DIR" ]; then
    NEED_SUDO="sudo"
    if ! command -v sudo &> /dev/null; then
        error "Cannot write to $INSTALL_DIR and sudo is not available"
    fi
fi

info "Pi Under Pressure Installer"
info "Architecture: $ARCH"
info "Install directory: $INSTALL_DIR"

# Get latest release
info "Fetching latest release..."
LATEST_RELEASE=$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_RELEASE" ]; then
    error "Failed to fetch latest release"
fi

info "Latest version: $LATEST_RELEASE"

# Download binary
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_RELEASE}/${ASSET_NAME}"
TEMP_FILE=$(mktemp)

info "Downloading from: $DOWNLOAD_URL"
if ! curl -sSL -o "$TEMP_FILE" "$DOWNLOAD_URL"; then
    rm -f "$TEMP_FILE"
    error "Failed to download binary"
fi

# Make executable
chmod +x "$TEMP_FILE"

# Install
info "Installing to $INSTALL_DIR/$BINARY_NAME..."
$NEED_SUDO mkdir -p "$INSTALL_DIR"
$NEED_SUDO mv "$TEMP_FILE" "$INSTALL_DIR/$BINARY_NAME"

# Verify installation
if command -v "$BINARY_NAME" &> /dev/null; then
    info "Installation successful!"
    echo ""
    $BINARY_NAME --version
    echo ""
    info "Run '$BINARY_NAME --help' for usage information"
else
    warn "Binary installed but not in PATH"
    info "Add $INSTALL_DIR to your PATH or run: $INSTALL_DIR/$BINARY_NAME"
fi
