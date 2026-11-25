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
  ENGRAVER_VERSION
                Version to install, such as 0.1.0 (default: the latest release)
  ENGRAVER_RELEASE_URL
                Download release files from <url>/v<version>/<file>
                (default: https://github.com/mstephenholl/engraver/releases/download)

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

# Refuse a downloaded file unless its SHA-256 is the one checksums.txt lists.
# Runs in the main shell, so that error ends the script.
verify_checksum() {
    local file="$1"
    local checksums="$2"
    local name expected actual

    name=$(basename "$file")
    expected=$(awk -v name="$name" '$2 == name { print $1; exit }' "$checksums")
    if [[ -z "$expected" ]]; then
        error "checksums.txt has no entry for ${name}."
    fi

    if command -v sha256sum &>/dev/null; then
        actual=$(sha256sum "$file")
    elif command -v shasum &>/dev/null; then
        actual=$(shasum -a 256 "$file")
    else
        error "Cannot check the download: neither sha256sum nor shasum is installed."
    fi
    actual="${actual%% *}"

    if [[ "$actual" != "$expected" ]]; then
        error "Checksum mismatch for ${name}: checksums.txt says ${expected}, the download is ${actual}."
    fi
}

# Download and install
install_binary() {
    local version="$1"
    local platform="$2"
    local archive="engraver-v${version}-${platform}.tar.gz"
    local release_url="${ENGRAVER_RELEASE_URL:-https://github.com/${REPO}/releases/download}/v${version}"
    local tmpdir
    
    tmpdir=$(mktemp -d)
    # shellcheck disable=SC2064  # Intentional: expand tmpdir now, not at trap time
    trap "rm -rf $tmpdir" EXIT

    info "Downloading engraver v${version} for ${platform}..."
    curl -fsSL "${release_url}/${archive}" -o "${tmpdir}/${archive}" ||
        error "Could not download ${release_url}/${archive}"
    curl -fsSL "${release_url}/checksums.txt" -o "${tmpdir}/checksums.txt" ||
        error "Could not download ${release_url}/checksums.txt"

    verify_checksum "${tmpdir}/${archive}" "${tmpdir}/checksums.txt"
    tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"
    
    local extract_dir="${tmpdir}/engraver-v${version}-${platform}"
    
    info "Installing to ${INSTALL_DIR}..."
    if [[ -w "$INSTALL_DIR" ]]; then
        cp "${extract_dir}/engraver" "$INSTALL_DIR/"
    elif [[ "$USE_SUDO" == "true" ]]; then
        sudo cp "${extract_dir}/engraver" "$INSTALL_DIR/"
    else
        error "Cannot write to ${INSTALL_DIR}. Set INSTALL_DIR to a writable location or remove --no-sudo."
    fi
    
    # Install completions
    install_completions "$extract_dir"
    
    # Install man pages
    install_man_pages "$extract_dir"
}

# copy_files <what> <dest> <file>...: copy the files to dest, a directory or a
# file path, then say whether that worked. When the directory isn't writable,
# copy through sudo, unless --no-sudo is set or the directory is under HOME.
copy_files() {
    local what="$1" dest="$2"
    shift 2
    local dir="$dest"
    [[ -d "$dest" ]] || dir="$(dirname "$dest")"

    local copied=false
    if [[ -w "$dir" ]]; then
        cp "$@" "$dest" 2>/dev/null && copied=true
    elif [[ "$USE_SUDO" == "true" && "$dir" != "$HOME"/* ]]; then
        sudo cp "$@" "$dest" 2>/dev/null && copied=true
    fi

    if [[ "$copied" == true ]]; then
        info "Installed ${what} to ${dest}"
    else
        warn "Skipped ${what}: could not write to ${dir}"
    fi
}

install_completions() {
    local src_dir="$1"

    # Bash
    if [[ -d /usr/local/share/bash-completion/completions ]]; then
        copy_files "bash completions" /usr/local/share/bash-completion/completions/engraver "${src_dir}/completions/engraver.bash"
    elif [[ -d /etc/bash_completion.d ]]; then
        copy_files "bash completions" /etc/bash_completion.d/engraver "${src_dir}/completions/engraver.bash"
    fi

    # Zsh
    if [[ -d /usr/local/share/zsh/site-functions ]]; then
        copy_files "zsh completions" /usr/local/share/zsh/site-functions/_engraver "${src_dir}/completions/_engraver"
    fi

    # Fish
    if [[ -d ~/.config/fish/completions ]]; then
        copy_files "fish completions" ~/.config/fish/completions/engraver.fish "${src_dir}/completions/engraver.fish"
    fi

    # Elvish
    if [[ -d ~/.elvish/lib ]]; then
        copy_files "elvish completions" ~/.elvish/lib/engraver.elv "${src_dir}/completions/engraver.elv"
    fi

    # PowerShell
    local ps_dir
    # $PROFILE is a PowerShell variable, not a shell variable
    # shellcheck disable=SC2016
    if ps_dir="$(pwsh -NoProfile -Command 'Split-Path $PROFILE' 2>/dev/null)"; then
        if [[ -d "$ps_dir" ]]; then
            copy_files "PowerShell completions" "${ps_dir}/engraver.ps1" "${src_dir}/completions/_engraver.ps1"
        fi
    fi
}

install_man_pages() {
    local src_dir="$1"

    # Try to create MAN_DIR if needed
    if [[ ! -d "$MAN_DIR" ]]; then
        if [[ "$USE_SUDO" == "true" ]]; then
            sudo mkdir -p "$MAN_DIR" 2>/dev/null || true
        else
            mkdir -p "$MAN_DIR" 2>/dev/null || true
        fi
    fi
    if [[ ! -d "$MAN_DIR" ]]; then
        warn "Skipped man pages: could not create ${MAN_DIR}"
        return
    fi

    copy_files "man pages" "$MAN_DIR" "${src_dir}"/man/*.1
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
    
    # --target-dir beats CARGO_TARGET_DIR, so the binary is in ./target even
    # when the user has pointed cargo somewhere else.
    cargo build --release -p engraver-cli --target-dir target
    
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
    ./target/release/engraver completions powershell > completions/_engraver.ps1
    ./target/release/engraver completions elvish > completions/engraver.elv
    
    # The clone's man/ holds the curated pages. Their .TH lines name the version
    # they were last edited for, so stamp the built version in. `sed -i.bak` is
    # the form GNU and BSD sed share.
    local version
    version=$(./target/release/engraver --version | awk '{ print $2 }')
    sed -i.bak -E "/^\.TH /s/\"engraver [^\"]*\"/\"engraver ${version}\"/" man/*.1
    rm man/*.1.bak

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
    
    # Install the version asked for, or else the latest release from GitHub
    if [[ -n "${ENGRAVER_VERSION:-}" ]]; then
        version="${ENGRAVER_VERSION#v}"
        info "Version: v${version}"
        install_binary "$version" "$platform"
    elif version=$(get_latest_version 2>/dev/null) && [[ -n "$version" ]]; then
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
