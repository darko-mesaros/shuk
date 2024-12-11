#!/bin/sh
# Universal installer script
set -e

# REQUIREMENTS:
# curl

# Configuration
GITHUB_REPO="darko-mesaros/shuk"
BINARY_NAME="shuk"
INSTALL_DIR="$HOME/.local/bin"

# Print error and exit
error() {
    echo "Error: $1"
    exit 1
}

# Detect OS and architecture
detect_platform() {
    local os arch

    # Detect OS
    case "$(uname -s)" in
        Linux)
            os="unknown-linux-musl"
            ;;
        Darwin)
            os="apple-darwin"
            ;;
        MINGW*|MSYS*|CYGWIN*)
            os="pc-windows-msvc"
            ;;
        *)
            error "unsupported OS: $(uname -s)"
            ;;
    esac

    # Detect architecture
    case "$(uname -m)" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        arm64|aarch64)
            arch="aarch64"
            ;;
        *)
            error "unsupported architecture: $(uname -m)"
            ;;
    esac

    echo "${arch}-${os}"
}

# Get latest version from GitHub
get_latest_version() {
    curl --silent "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" |
    grep '"tag_name":' |
    sed -E 's/.*"([^"]+)".*/\1/'
}

# Download and install the binary
install() {
    local platform="$1"
    local version="$2"
    local tmp_dir
    local ext

    # Determine file extension
    case "$platform" in
        *windows*)
            ext=".exe"
            ;;
        *)
            ext=""
            ;;
    esac

    # Create temporary directory
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    echo "Downloading ${BINARY_NAME} ${version} for ${platform}..."
    
    # Download and extract
    # https://github.com/darko-mesaros/shuk/releases/download/v0.4.6/shuk-x86_64-unknown-linux-gnu.tar.gz
    curl -sL "https://github.com/${GITHUB_REPO}/releases/download/${version}/${BINARY_NAME}-${platform}.tar.gz" |
    tar xz -C "$tmp_dir"

    # Create install directory if it doesn't exist
    mkdir -p "$INSTALL_DIR"

    # Install binary
    mv "${tmp_dir}/${BINARY_NAME}${ext}" "${INSTALL_DIR}/${BINARY_NAME}${ext}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}${ext}"

    echo "Successfully installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}${ext}"
}

# Check if curl is available
command -v curl >/dev/null 2>&1 || error "curl is required but not installed"

# Detect platform
PLATFORM=$(detect_platform)

# Get latest version if not specified
VERSION=${1:-$(get_latest_version)}

# Install
install "$PLATFORM" "$VERSION"

# Add to PATH instructions
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
        echo "
Please add ${INSTALL_DIR} to your PATH:
    setx PATH \"%PATH%;${INSTALL_DIR}\"
"
        ;;
    *)
        echo "
Please add ${INSTALL_DIR} to your PATH:
    export PATH=\"\$PATH:${INSTALL_DIR}\"

You can add this line to your ~/.bashrc or ~/.zshrc file to make it permanent.
"
        ;;
esac
