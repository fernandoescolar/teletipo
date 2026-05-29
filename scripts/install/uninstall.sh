#!/bin/sh
set -eu

PREFIX="${HOME}/.local"
REMOVE_DESKTOP="yes"

usage() {
    cat <<'EOF'
Usage: uninstall.sh [options]

Options:
  --prefix <dir>   Installation prefix (default: ~/.local)
  --no-desktop     Do not remove desktop integration files
  -h, --help       Show this help
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix)
            PREFIX="$2"
            shift 2
            ;;
        --no-desktop)
            REMOVE_DESKTOP="no"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf '%s\n' "[teletipo] unknown option: $1" >&2
            exit 1
            ;;
    esac
done

rm -f "${PREFIX}/bin/teletipo"

if [ "$REMOVE_DESKTOP" = "yes" ]; then
    rm -f "${PREFIX}/share/applications/teletipo.desktop"
    rm -f "${PREFIX}/share/icons/hicolor/128x128/apps/teletipo.png"
    rm -rf "/Applications/Teletipo.app"
    rm -rf "${HOME}/Applications/Teletipo.app"
fi

printf '%s\n' "[teletipo] uninstall complete"
