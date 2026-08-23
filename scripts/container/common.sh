#!/usr/bin/env bash

CONTAINER_SCRIPT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPOSITORY_ROOT="$(cd -- "$CONTAINER_SCRIPT_DIRECTORY/../.." && pwd)"

build_image() {
    local engine="$1"
    local image="$2"

    "$engine" build --pull \
        --file "$REPOSITORY_ROOT/Containerfile" \
        --tag "$image" \
        "$REPOSITORY_ROOT"
}

run_make_target() {
    local engine="$1"
    local image="$2"
    local make_target="$3"

    "$engine" run --rm \
        "$image" \
        make "$make_target"
}

run_verification() {
    run_make_target "$1" "$2" verify
}

run_tests() {
    run_make_target "$1" "$2" test
}

package_release() {
    local engine="$1"
    local image="$2"
    local version="$3"
    local release_evidence="$REPOSITORY_ROOT/benchmarks/releases/$version"
    local dist_directory="$REPOSITORY_ROOT/dist"

    if [ -e "$release_evidence" ] && [ ! -d "$release_evidence" ]; then
        echo "benchmark release path is not a directory: $release_evidence" >&2
        return 1
    fi

    mkdir -p "$dist_directory"

    local volumes=(--volume "$dist_directory:/workspace/dist")
    if [ -d "$release_evidence" ]; then
        volumes+=(
            --volume "$REPOSITORY_ROOT/benchmarks/releases:/workspace/benchmarks/releases:ro"
        )
    fi

    "$engine" run --rm "${volumes[@]}" \
        "$image" \
        make package VERSION="$version"
}
