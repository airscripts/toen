#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 2 ]; then
    echo "usage: $0 <tag> <output-file>" >&2
    exit 2
fi

tag="$1"
output_file="$2"

if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "release tag must use stable vX.Y.Z format: $tag" >&2
    exit 1
fi

version="${tag#v}"
declared_version="$(tr -d '[:space:]' < VERSION)"

if [ "$version" != "$declared_version" ]; then
    echo "release tag $tag does not match VERSION $declared_version" >&2
    exit 1
fi

valid_calendar_date() {
    local date="$1"
    local year month day max_day

    [[ "$date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || return 1

    year=$((10#${date:0:4}))
    month=$((10#${date:5:2}))
    day=$((10#${date:8:2}))
    (( year >= 1 && month >= 1 && month <= 12 )) || return 1

    case "$month" in
        2)
            max_day=28
            if (( year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) )); then
                max_day=29
            fi
            ;;
        4|6|9|11) max_day=30 ;;
        *) max_day=31 ;;
    esac

    (( day >= 1 && day <= max_day ))
}

heading_prefix="## [$version] - "
section_found=false
section_date=""
body=()
body_entry=false
changelog_lines=()

while IFS= read -r line || [ -n "$line" ]; do
    changelog_lines+=("$line")
done < CHANGELOG.md

matching_sections=0
for line in "${changelog_lines[@]}"; do
    if [[ "$line" == "$heading_prefix"* ]]; then
        matching_sections=$((matching_sections + 1))
    fi
done

if [ "$matching_sections" -gt 1 ]; then
    echo "CHANGELOG.md contains duplicate sections for $version" >&2
    exit 1
fi

for line in "${changelog_lines[@]}"; do
    if [[ "$line" == "$heading_prefix"* ]]; then
        section_found=true
        section_date="${line#"$heading_prefix"}"
        continue
    fi

    if [ "$section_found" = true ] && [[ "$line" == "## ["* ]]; then
        break
    fi

    if [ "$section_found" = true ]; then
        body+=("$line")
        if [[ "$line" =~ ^[[:space:]]*-[[:space:]]+ ]]; then
            body_entry=true
        fi
    fi
done

if [ "$section_found" != true ]; then
    echo "CHANGELOG.md section for $version is missing" >&2
    exit 1
fi

if ! valid_calendar_date "$section_date"; then
    echo "CHANGELOG.md section for $version must use a valid YYYY-MM-DD date" >&2
    exit 1
fi

if [ "$body_entry" != true ]; then
    echo "CHANGELOG.md section for $version must contain a changelog entry" >&2
    exit 1
fi

{
    printf '## toen@%s | %s\n\n' "$tag" "$section_date"
    body_started=false
    for line in "${body[@]}"; do
        if [ "$body_started" != true ] && [ -z "$line" ]; then
            continue
        fi
        body_started=true
        printf '%s\n' "$line"
    done
} > "$output_file"
