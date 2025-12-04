#!/bin/bash
set -e

# Hotel Broken Image Checker Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/dijii-tech/hotel-broken-image-checker/main/install.sh | bash

REPO="dijii-tech/hotel-broken-image-checker"
BINARY_NAME="hotel-broken-image-checker"
INSTALL_DIR="/usr/local/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Detect OS and Architecture
detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        linux)
            case "$ARCH" in
                x86_64)  PLATFORM="linux-x86_64" ;;
                aarch64) PLATFORM="linux-aarch64" ;;
                arm64)   PLATFORM="linux-aarch64" ;;
                *)       error "Unsupported architecture: $ARCH" ;;
            esac
            ;;
        darwin)
            case "$ARCH" in
                x86_64)  PLATFORM="macos-x86_64" ;;
                arm64)   PLATFORM="macos-aarch64" ;;
                *)       error "Unsupported architecture: $ARCH" ;;
            esac
            ;;
        *)
            error "Unsupported OS: $OS"
            ;;
    esac

    info "Detected platform: $PLATFORM"
}

# Get latest release version
get_latest_version() {
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
        error "Failed to get latest version"
    fi
    info "Latest version: $VERSION"
}

# Download and install
install() {
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$BINARY_NAME-$PLATFORM.tar.gz"
    TMP_DIR=$(mktemp -d)

    info "Downloading from: $DOWNLOAD_URL"

    if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$BINARY_NAME.tar.gz"; then
        error "Failed to download binary"
    fi

    info "Extracting..."
    tar -xzf "$TMP_DIR/$BINARY_NAME.tar.gz" -C "$TMP_DIR"

    info "Installing to $INSTALL_DIR..."
    if [ -w "$INSTALL_DIR" ]; then
        mv "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
    else
        sudo mv "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
    fi

    chmod +x "$INSTALL_DIR/$BINARY_NAME"

    # Cleanup
    rm -rf "$TMP_DIR"

    info "Installed successfully!"
}

# Verify installation
verify() {
    if command -v "$BINARY_NAME" &> /dev/null; then
        echo ""
        info "Installation complete!"
        echo ""
        echo "  Version: $($BINARY_NAME --version 2>/dev/null || echo $VERSION)"
        echo "  Location: $(which $BINARY_NAME)"
        echo ""
        echo "  Usage:"
        echo "    $BINARY_NAME --help"
        echo ""
    else
        warn "Binary installed but not in PATH. Add $INSTALL_DIR to your PATH."
    fi
}

main() {
    echo ""
    echo "  Hotel Broken Image Checker Installer"
    echo "  ====================================="
    echo ""

    detect_platform
    get_latest_version
    install
    verify
}

main
