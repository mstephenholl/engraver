#!/bin/bash
# Engraver installation script
# Version: 1.0.0
# Usage: curl -fsSL https://raw.githubusercontent.com/mstephenholl/engraver/main/install.sh | bash
#
# Options:
#   --version     Show installer version
#   --help        Show this help message
#   --no-sudo     Don't use sudo (install to user directory)

set -e

INSTALLER_VERSION="1.0.0"
REPO="mstephenholl/engraver"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
COMPLETIONS_DIR="${COMPLETIONS_DIR:-}"
MAN_DIR="${MAN_DIR:-/usr/local/share/man/man1}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Show help
show_help() {
    cat << EOF
Engraver Installer v${INSTALLER_VERSION}

Usage: curl -fsSL https://raw.githubusercontent.com/mstephenholl/engraver/main/install.sh | bash

Or download and run locally:
  curl -fsSL https://raw.githubusercontent.com/mstephenholl/engraver/main/install.sh -o install.sh
  bash install.sh [OPTIONS]

Options:
  --version     Show installer version
  --help        Show this help message
  --no-sudo     Don't use sudo (requires writable INSTALL_DIR)

Environment Variables:
  INSTALL_DIR   Installation directory (default: /usr/local/bin)
  MAN_DIR       Man page directory (default: /usr/local/share/man/man1)

EOF
    exit 0
}

# Show version
show_version() {
    echo "Engraver Installer v${INSTALLER_VERSION}"
    exit 0
}

# Parse command line arguments
USE_SUDO=true
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --version)
                show_version
                ;;
            --help|-h)
                show_help
                ;;
            --no-sudo)
                USE_SUDO=false
                shift
                ;;
            *)
                error "Unknown option: $1. Use --help for usage."
                ;;
        esac
    done
}

# Detect OS and architecture
detect_platform() {
    local os arch
    
    case "$(uname -s)" in
        Linux*)  os="unknown-linux-gnu";;
        Darwin*) os="apple-darwin";;
        MINGW*|MSYS*|CYGWIN*) os="pc-windows-msvc";;
        *) error "Unsupported operating system: $(uname -s)";;
    esac
    
    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64";;
        aarch64|arm64) arch="aarch64";;
        *) error "Unsupported architecture: $(uname -m)";;
    esac
    
    echo "${arch}-${os}"
}

# Get latest release version
get_latest_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | \
        grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/'
}

# Download and install
install_binary() {
    local version="$1"
    local platform="$2"
    local url="https://github.com/${REPO}/releases/download/v${version}/engraver-v${version}-${platform}.tar.gz"
    local tmpdir
    
    tmpdir=$(mktemp -d)
    # shellcheck disable=SC2064  # Intentional: expand tmpdir now, not at trap time
    trap "rm -rf $tmpdir" EXIT

    info "Downloading engraver v${version} for ${platform}..."
    curl -fsSL "$url" | tar -xz -C "$tmpdir"
    
    local extract_dir="${tmpdir}/engraver-v${version}-${platform}"
    
    info "Installing to ${INSTALL_DIR}..."
    if [[ -w "$INSTALL_DIR" ]]; then
        cp "${extract_dir}/engraver" "$INSTALL_DIR/"
    elif [[ "$USE_SUDO" == "true" ]]; then
        sudo cp "${extract_dir}/engraver" "$INSTALL_DIR/"
    else
        error "Cannot write to ${INSTALL_DIR}. Set INSTALL_DIR to a writable location or remove --no-sudo."
    fi
    chmod +x "${INSTALL_DIR}/engraver"
    
    # Install completions
    install_completions "$extract_dir"
    
    # Install man pages
    install_man_pages "$extract_dir"
}

install_completions() {
    local src_dir="$1"
    
    # Helper to copy with optional sudo
    maybe_sudo_cp() {
        if [[ -w "$(dirname "$2")" ]]; then
            cp "$1" "$2" 2>/dev/null || true
        elif [[ "$USE_SUDO" == "true" ]]; then
            sudo cp "$1" "$2" 2>/dev/null || true
        fi
    }

    # Bash
    if [[ -d /usr/local/share/bash-completion/completions ]]; then
        info "Installing bash completions..."
        maybe_sudo_cp "${src_dir}/completions/engraver.bash" /usr/local/share/bash-completion/completions/engraver
    elif [[ -d /etc/bash_completion.d ]]; then
        maybe_sudo_cp "${src_dir}/completions/engraver.bash" /etc/bash_completion.d/engraver
    fi

    # Zsh
    if [[ -d /usr/local/share/zsh/site-functions ]]; then
        info "Installing zsh completions..."
        maybe_sudo_cp "${src_dir}/completions/_engraver" /usr/local/share/zsh/site-functions/_engraver
    fi
    
    # Fish
    if [[ -d ~/.config/fish/completions ]]; then
        info "Installing fish completions..."
        cp "${src_dir}/completions/engraver.fish" ~/.config/fish/completions/ 2>/dev/null || true
    fi
}

install_man_pages() {
    local src_dir="$1"

    # Try to create MAN_DIR if needed
    if [[ ! -d "$MAN_DIR" ]]; then
        if [[ "$USE_SUDO" == "true" ]]; then
            sudo mkdir -p "$MAN_DIR" 2>/dev/null || return
        else
            mkdir -p "$MAN_DIR" 2>/dev/null || return
        fi
    fi

    if [[ -d "$MAN_DIR" ]]; then
        info "Installing man pages..."
        if [[ -w "$MAN_DIR" ]]; then
            cp "${src_dir}"/man/*.1 "$MAN_DIR/" 2>/dev/null || true
        elif [[ "$USE_SUDO" == "true" ]]; then
            sudo cp "${src_dir}"/man/*.1 "$MAN_DIR/" 2>/dev/null || true
        fi
    fi
}

# Build from source
install_from_source() {
    info "Building from source..."
    
    if ! command -v cargo &>/dev/null; then
        error "Rust/Cargo not found. Install from https://rustup.rs"
    fi
    
    local tmpdir
    tmpdir=$(mktemp -d)
    # shellcheck disable=SC2064  # Intentional: expand tmpdir now, not at trap time
    trap "rm -rf $tmpdir" EXIT

    git clone "https://github.com/${REPO}.git" "$tmpdir/engraver"
    cd "$tmpdir/engraver"
    
    cargo build --release -p engraver
    
    info "Installing to ${INSTALL_DIR}..."
    if [[ -w "$INSTALL_DIR" ]]; then
        cp target/release/engraver "$INSTALL_DIR/"
    elif [[ "$USE_SUDO" == "true" ]]; then
        sudo cp target/release/engraver "$INSTALL_DIR/"
    else
        error "Cannot write to ${INSTALL_DIR}. Set INSTALL_DIR to a writable location or remove --no-sudo."
    fi
    
    # Generate and install completions
    mkdir -p completions
    ./target/release/engraver completions bash > completions/engraver.bash
    ./target/release/engraver completions zsh > completions/_engraver
    ./target/release/engraver completions fish > completions/engraver.fish
    
    mkdir -p man
    ./target/release/engraver mangen --out-dir man
    
    install_completions "."
    install_man_pages "."
}

main() {
    parse_args "$@"

    echo "╔════════════════════════════════════════╗"
    echo "║     Engraver Installer v${INSTALLER_VERSION}            ║"
    echo "╚════════════════════════════════════════╝"
    echo

    local platform version
    platform=$(detect_platform)
    
    # Try to get latest version from GitHub
    if version=$(get_latest_version 2>/dev/null) && [[ -n "$version" ]]; then
        info "Latest version: v${version}"
        install_binary "$version" "$platform"
    else
        warn "Could not fetch latest release, building from source..."
        install_from_source
    fi
    
    echo
    info "Installation complete!"
    echo
    echo "Get started:"
    echo "  engraver list           # List available drives"
    echo "  engraver --help         # Show all commands"
    echo
    echo "Note: Writing to devices requires root privileges:"
    echo "  sudo engraver write <image> <device>"
}

main "$@"
