#!/usr/bin/env bash
set -euo pipefail

script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
release_notes="$script_directory/release-notes.sh"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/toen-release-notes.XXXXXX")"
trap 'rm -rf -- "$test_root"' EXIT

make_case() {
    local name="$1"
    shift
    local directory="$test_root/$name"
    mkdir -p "$directory"
    printf '0.1.0\n' > "$directory/VERSION"
    printf '%s\n' "$@" > "$directory/CHANGELOG.md"
    printf '%s' "$directory"
}

expect_failure() {
    local directory="$1"
    local tag="$2"
    local output="$directory/release-notes.md"

    if (cd "$directory" && "$release_notes" "$tag" "$output") >/dev/null 2>&1; then
        echo "expected release-notes.sh to reject $directory" >&2
        exit 1
    fi
    if [ -e "$output" ]; then
        echo "release-notes.sh left output after rejecting $directory" >&2
        exit 1
    fi
}

stable_case="$(make_case stable \
    '# Changelog' \
    '' \
    '## [Unreleased]' \
    '' \
    '## [0.1.0] - 2026-08-23' \
    '' \
    '### Added' \
    '' \
    '- Initial stable release.')"
(cd "$stable_case" && "$release_notes" v0.1.0 release-notes.md)
grep -Fq '## toen@v0.1.0 | 2026-08-23' "$stable_case/release-notes.md"
grep -Fq -- '- Initial stable release.' "$stable_case/release-notes.md"

prerelease_case="$(make_case prerelease \
    '## [0.1.0] - 2026-08-23' \
    '' \
    '- Stable entry.')"
expect_failure "$prerelease_case" v0.1.0-rc.1

empty_case="$(make_case empty \
    '## [0.1.0] - 2026-08-23' \
    '' \
    '### Added')"
expect_failure "$empty_case" v0.1.0

missing_case="$(make_case missing \
    '## [Unreleased]' \
    '' \
    '- Work in progress.')"
expect_failure "$missing_case" v0.1.0

malformed_case="$(make_case malformed \
    '## [0.1.0] - 2026-8-23' \
    '' \
    '- Malformed date.')"
expect_failure "$malformed_case" v0.1.0

invalid_date_case="$(make_case invalid-date \
    '## [0.1.0] - 2026-02-30' \
    '' \
    '- Impossible date.')"
expect_failure "$invalid_date_case" v0.1.0

unreleased_case="$(make_case unreleased \
    '## [0.1.0] - Unreleased' \
    '' \
    '- Undated entry.')"
expect_failure "$unreleased_case" v0.1.0

separated_duplicate_case="$(make_case separated-duplicate \
    '## [0.1.0] - 2026-08-23' \
    '' \
    '- First entry.' \
    '' \
    '## [0.2.0] - 2027-01-01' \
    '' \
    '- Other release.' \
    '' \
    '## [0.1.0] - 2026-08-24' \
    '' \
    '- Duplicate entry.')"
expect_failure "$separated_duplicate_case" v0.1.0

echo "release-notes: stable, prerelease, empty, missing, malformed, invalid-date, Unreleased, and duplicate cases passed"
