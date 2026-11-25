#!/usr/bin/env bash
# Install a release's archive through install.sh, the way a user would.
#
# Usage: scripts/test-install.sh <dist-dir>
#
# <dist-dir> holds the files a release publishes: the engraver-<TAG>-<target>
# archives and checksums.txt. They are served from a local mirror laid out like
# GitHub's release downloads. INSTALL_DIR and MAN_DIR are owned by root, so
# install.sh has to copy through sudo.
#
# Case 1 installs the archive as built and expects a working install.
# Case 2 runs --no-sudo with a man page directory only root can write to, and
# expects install.sh not to claim the man pages it could not install.
# Case 3 changes every archive without changing what it extracts, and expects
# install.sh to refuse it.
#
# Environment:
#   TAG   release tag the archives are named for (default: v0.0.0-dry-run)
#   PORT  port of the mirror (default: 8765)

set -euo pipefail

TAG="${TAG:-v0.0.0-dry-run}"
PORT="${PORT:-8765}"

if [[ $# -ne 1 || ! -d "$1" ]]; then
    echo "Usage: $0 <dist-dir>" >&2
    exit 2
fi
if ! command -v sudo >/dev/null 2>&1; then
    echo "sudo is required: the test installs into root-owned directories" >&2
    exit 2
fi

dist="$(cd "$1" && pwd)"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

work="$(mktemp -d)"
server_pid=""
cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
    fi
    sudo rm -rf "$work"
}
trap cleanup EXIT

mirror="$work/mirror"
install_dir="$work/prefix/bin"
man_dir="$work/prefix/share/man/man1"
no_sudo_bin="$work/no-sudo/bin"
no_sudo_man_dir="$work/prefix/share/no-sudo-man/man1"
home="$work/home"

mkdir -p "$mirror/$TAG" "$home" "$no_sudo_bin"
cp "$dist"/* "$mirror/$TAG/"
if ! compgen -G "$mirror/$TAG/*.tar.gz" >/dev/null; then
    echo "No .tar.gz archive in $dist" >&2
    exit 2
fi

python3 -m http.server --bind 127.0.0.1 --directory "$mirror" "$PORT" >"$work/server.log" 2>&1 &
server_pid=$!
ready=false
for _ in {1..50}; do
    kill -0 "$server_pid" 2>/dev/null || break
    if curl -fsS -o /dev/null "http://127.0.0.1:$PORT/$TAG/checksums.txt" 2>/dev/null; then
        ready=true
        break
    fi
    sleep 0.2
done
if [[ "$ready" != true ]]; then
    echo "The mirror did not start on port $PORT:" >&2
    cat "$work/server.log" >&2
    exit 2
fi

# Only root can write to these directories, so install.sh has to copy through sudo.
sudo mkdir -p "$install_dir" "$man_dir" "$no_sudo_man_dir"
sudo chown -R 0:0 "$work/prefix"
sudo chmod -R 755 "$work/prefix"
if [[ -w "$install_dir" || -w "$man_dir" || -w "$no_sudo_man_dir" ]]; then
    echo "This user can still write to the install directories, so the sudo path would go untested:" >&2
    ls -ld "$install_dir" "$man_dir" "$no_sudo_man_dir" >&2
    exit 2
fi

# run_install <install-dir> <man-dir> [install.sh option]...
# HOME is a throwaway so install.sh cannot add completions to the real one.
run_install() {
    local bin="$1" man="$2"
    shift 2
    INSTALL_DIR="$bin" MAN_DIR="$man" HOME="$home" \
        ENGRAVER_RELEASE_URL="http://127.0.0.1:$PORT" ENGRAVER_VERSION="${TAG#v}" \
        bash "$repo/install.sh" "$@"
}

failures=0

# verdict <name> <problems> <install.sh output>: print PASS or FAIL, then the output.
verdict() {
    local name="$1" problems="$2" output="$3"
    if [[ -z "$problems" ]]; then
        echo "PASS: $name"
    else
        echo "FAIL: $name"
        printf '%s' "$problems"
        failures=$((failures + 1))
    fi
    while IFS= read -r line; do
        printf '    | %s\n' "$line"
    done <<<"$output"
    echo
}

# Case 1: the archive as built
status=0
output="$(run_install "$install_dir" "$man_dir" 2>&1)" || status=$?
problems=""
if [[ "$status" -ne 0 ]]; then
    problems+="    install.sh exited with status $status"$'\n'
fi
if ! "$install_dir/engraver" --version >/dev/null 2>&1; then
    problems+="    $install_dir/engraver --version does not run"$'\n'
fi
if [[ ! -e "$man_dir/engraver.1" ]]; then
    problems+="    $man_dir/engraver.1 is missing"$'\n'
fi
verdict "install from a good archive" "$problems" "$output"

# Case 2: --no-sudo, so the man pages can't go into a directory only root can
# write to. install.sh must still install the binary and must not say it
# installed the man pages.
status=0
output="$(run_install "$no_sudo_bin" "$no_sudo_man_dir" --no-sudo 2>&1)" || status=$?
problems=""
if [[ "$status" -ne 0 ]]; then
    problems+="    install.sh exited with status $status"$'\n'
fi
if ! "$no_sudo_bin/engraver" --version >/dev/null 2>&1; then
    problems+="    $no_sudo_bin/engraver --version does not run"$'\n'
fi
if grep -qE 'Install(ed|ing) man pages' <<<"$output"; then
    problems+="    install.sh said it installed the man pages, but $no_sudo_man_dir is not writable"$'\n'
fi
verdict "say what --no-sudo skipped" "$problems" "$output"

# Case 3: a corrupted archive. An empty gzip member at the end changes the
# file but not what gzip and tar extract from it, so only the checksum can tell.
sudo rm -f "$install_dir/engraver"
for archive in "$mirror/$TAG"/*.tar.gz; do
    gzip -n </dev/null >>"$archive"
done
status=0
output="$(run_install "$install_dir" "$man_dir" 2>&1)" || status=$?
problems=""
if [[ "$status" -eq 0 ]]; then
    problems+="    install.sh accepted the archive and exited 0"$'\n'
fi
if [[ -e "$install_dir/engraver" ]]; then
    problems+="    $install_dir/engraver was installed from the corrupted archive"$'\n'
fi
verdict "refuse a corrupted archive" "$problems" "$output"

if [[ "$failures" -ne 0 ]]; then
    echo "$failures case(s) failed" >&2
    exit 1
fi
echo "All cases passed"
