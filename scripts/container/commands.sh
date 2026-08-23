#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=common.sh
. "$SCRIPT_DIRECTORY/common.sh"

usage() {
    cat >&2 <<'EOF'
usage:
  commands.sh build <engine> <image>
  commands.sh verify <engine> <image>
  commands.sh test <engine> <image>
  commands.sh run <engine> <image> <make-target>
  commands.sh package <engine> <image> <version>
EOF
}

main() {
    local command="${1:-}"

    if [ "$#" -gt 0 ]; then
        shift
    fi

    case "$command" in
        build)
            [ "$#" -eq 2 ] || { usage; return 2; }
            build_image "$@"
            ;;
        verify)
            [ "$#" -eq 2 ] || { usage; return 2; }
            run_verification "$@"
            ;;
        test)
            [ "$#" -eq 2 ] || { usage; return 2; }
            run_tests "$@"
            ;;
        run)
            [ "$#" -eq 3 ] || { usage; return 2; }
            run_make_target "$@"
            ;;
        package)
            [ "$#" -eq 3 ] || { usage; return 2; }
            package_release "$@"
            ;;
        *)
            usage
            return 2
            ;;
    esac
}

main "$@"
