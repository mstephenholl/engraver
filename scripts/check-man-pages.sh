#!/usr/bin/env bash
# Check that the man pages under man/ read cleanly and name the current version.
#
# Usage: scripts/check-man-pages.sh
#
# A page fails when groff reads it with a warning, or when its .TH line does
# not name "engraver <version>", the version of engraver-cli. Every problem is
# reported as a GitHub ::error line. All pages are checked before the script
# exits 1. Needs groff, cargo and jq.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if ! command -v groff >/dev/null 2>&1; then
    echo "groff is required to check the man pages" >&2
    exit 2
fi

version="$(cargo metadata --no-deps --format-version 1 \
    | jq -er '.packages[] | select(.name == "engraver-cli") | .version')"

shopt -s nullglob
pages=(man/*.1)
if [[ ${#pages[@]} -eq 0 ]]; then
    echo "no man pages found in man/" >&2
    exit 2
fi

failed=0
for page in "${pages[@]}"; do
    # groff reports every warning on stderr; -z suppresses the rendered page.
    warnings="$(groff -man -Tutf8 -ww -z "$page" 2>&1 || true)"
    while IFS= read -r line; do
        [[ -n "$line" ]] || continue
        echo "::error file=$page::groff: $line"
        failed=1
    done <<<"$warnings"

    th="$(grep -m1 '^\.TH ' "$page" || true)"
    if [[ "$th" != *"\"engraver ${version}\""* ]]; then
        echo "::error file=$page::the .TH line does not name \"engraver ${version}\": ${th:-no .TH line}"
        failed=1
    fi
done

if [[ $failed -ne 0 ]]; then
    exit 1
fi
echo "${#pages[@]} man pages read cleanly and name engraver ${version}"
