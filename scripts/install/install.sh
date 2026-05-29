#!/bin/sh
set -eu

REPO="fernandoescolar/teletipo"
BASE_RELEASE_URL="https://github.com/${REPO}/releases"
VERSION="latest"
PREFIX="${HOME}/.local"
INSTALL_SCOPE="user"
ENABLE_DESKTOP="auto"
VERIFY="yes"
FROM_ARCHIVE=""
DO_UNINSTALL="no"

usage() {
    cat <<'EOF'
Usage: install.sh [options]

Options:
  --version <tag>     Install a specific tag (example: v0.1.0). Default: latest
  --prefix <dir>      Install prefix for CLI binary. Default: ~/.local
  --system            Install in /usr/local (requires sudo for write access)
  --desktop           Install desktop integration (.desktop/.app)
  --no-desktop        Skip desktop integration
  --no-verify         Skip SHA256 verification (not recommended)
  --from-archive <p>  Install from already extracted archive path
  --uninstall         Run uninstall flow
  -h, --help          Show this help
EOF
}

log() {
    printf '%s\n' "[teletipo] $*"
}

have_cmd() {
    command -v "$1" >/dev/null 2>&1
}

die() {
    printf '%s\n' "[teletipo] ERROR: $*" >&2
    exit 1
}

resolve_script_dir() {
    case "$0" in
        /*) dirname "$0" ;;
        *)
            if [ -n "${PWD:-}" ]; then
                dirname "${PWD}/$0"
            else
                printf '.'
            fi
            ;;
    esac
}

get_latest_tag() {
    final_url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "${BASE_RELEASE_URL}/latest")" || return 1
    basename "$final_url"
}

download_to() {
    url="$1"
    dest="$2"
    if have_cmd curl; then
        curl -fsSL "$url" -o "$dest"
    elif have_cmd wget; then
        wget -qO "$dest" "$url"
    else
        die "curl or wget is required"
    fi
}

ensure_write_dir() {
    target_dir="$1"
    mkdir -p "$target_dir"
    if [ ! -w "$target_dir" ] && ! have_cmd sudo; then
        die "cannot write to ${target_dir} and sudo is not available"
    fi
}

verify_sha256() {
    archive_path="$1"
    sums_path="$2"
    asset_name="$3"

    sum_line="$(grep "  ${asset_name}$" "$sums_path" || true)"
    [ -n "$sum_line" ] || die "missing checksum for ${asset_name}"

    expected="$(printf '%s' "$sum_line" | awk '{print $1}')"
    if have_cmd sha256sum; then
        actual="$(sha256sum "$archive_path" | awk '{print $1}')"
    elif have_cmd shasum; then
        actual="$(shasum -a 256 "$archive_path" | awk '{print $1}')"
    else
        die "sha256sum or shasum is required for verification"
    fi

    [ "$expected" = "$actual" ] || die "checksum mismatch for ${asset_name}"
}

extract_archive() {
    archive_path="$1"
    out_dir="$2"
    mkdir -p "$out_dir"
    tar -xzf "$archive_path" -C "$out_dir"
}

linux_install_desktop() {
    root_dir="$1"
    bin_path="$2"

    share_root="${PREFIX}/share"
    apps_dir="${share_root}/applications"
    icons_dir="${share_root}/icons/hicolor/128x128/apps"

    mkdir -p "$apps_dir" "$icons_dir"

    if [ -f "$root_dir/share/icons/hicolor/128x128/apps/teletipo.png" ]; then
        cp "$root_dir/share/icons/hicolor/128x128/apps/teletipo.png" "$icons_dir/teletipo.png"
    elif [ -f "$root_dir/teletipo.png" ]; then
        cp "$root_dir/teletipo.png" "$icons_dir/teletipo.png"
    fi

    desktop_file="${apps_dir}/teletipo.desktop"
    cat > "$desktop_file" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Teletipo
Comment=Modern GPU-accelerated terminal
Exec=${bin_path}
Icon=teletipo
Terminal=false
Categories=System;TerminalEmulator;
StartupNotify=true
EOF

    if have_cmd update-desktop-database; then
        update-desktop-database "$apps_dir" >/dev/null 2>&1 || true
    fi
}

install_linux() {
    root_dir="$1"
    src_bin=""

    if [ -f "$root_dir/teletipo" ]; then
        src_bin="$root_dir/teletipo"
    elif [ -f "$root_dir/teletipo-linux-x86_64/teletipo" ]; then
        src_bin="$root_dir/teletipo-linux-x86_64/teletipo"
        root_dir="$root_dir/teletipo-linux-x86_64"
    else
        src_bin="$(find "$root_dir" -maxdepth 3 -type f -name teletipo | head -n1 || true)"
    fi

    [ -n "$src_bin" ] || die "teletipo binary not found in ${root_dir}"

    bin_dir="${PREFIX}/bin"
    mkdir -p "$bin_dir"
    dest_bin="${bin_dir}/teletipo"
    cp "$src_bin" "$dest_bin"
    chmod 0755 "$dest_bin"

    if [ "$ENABLE_DESKTOP" = "yes" ] || [ "$ENABLE_DESKTOP" = "auto" ]; then
        linux_install_desktop "$root_dir" "$dest_bin"
    fi

    log "installed ${dest_bin}"
}

install_macos_app() {
    root_dir="$1"
    app_src=""

    if [ -d "$root_dir/Teletipo.app" ]; then
        app_src="$root_dir/Teletipo.app"
    elif [ -d "$root_dir/teletipo-macos-app/Teletipo.app" ]; then
        app_src="$root_dir/teletipo-macos-app/Teletipo.app"
    fi
    [ -n "$app_src" ] || die "Teletipo.app not found in ${root_dir}"

    app_dest="/Applications/Teletipo.app"
    if [ ! -w /Applications ]; then
        app_dest="${HOME}/Applications/Teletipo.app"
        mkdir -p "${HOME}/Applications"
    fi

    rm -rf "$app_dest"
    cp -R "$app_src" "$app_dest"
    log "installed ${app_dest}"
}

install_macos_cli() {
    root_dir="$1"
    src_bin=""

    if [ -f "$root_dir/teletipo" ]; then
        src_bin="$root_dir/teletipo"
    elif [ -f "$root_dir/teletipo-macos-universal/teletipo" ]; then
        src_bin="$root_dir/teletipo-macos-universal/teletipo"
    else
        src_bin="$(find "$root_dir" -maxdepth 3 -type f -name teletipo | head -n1 || true)"
    fi

    [ -n "$src_bin" ] || die "teletipo binary not found in ${root_dir}"

    bin_dir="${PREFIX}/bin"
    mkdir -p "$bin_dir"
    dest_bin="${bin_dir}/teletipo"
    cp "$src_bin" "$dest_bin"
    chmod 0755 "$dest_bin"
    log "installed ${dest_bin}"
}

run_uninstall() {
    if [ "$DO_UNINSTALL" != "yes" ]; then
        return
    fi

    script_dir="$(resolve_script_dir)"
    if [ -x "$script_dir/uninstall.sh" ]; then
        exec "$script_dir/uninstall.sh" --prefix "$PREFIX"
    fi

    rm -f "${PREFIX}/bin/teletipo"
    rm -f "${PREFIX}/share/applications/teletipo.desktop"
    rm -f "${PREFIX}/share/icons/hicolor/128x128/apps/teletipo.png"
    rm -rf "${HOME}/Applications/Teletipo.app"
    log "uninstalled"
    exit 0
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            VERSION="$2"
            shift 2
            ;;
        --prefix)
            PREFIX="$2"
            shift 2
            ;;
        --system)
            INSTALL_SCOPE="system"
            PREFIX="/usr/local"
            shift
            ;;
        --desktop)
            ENABLE_DESKTOP="yes"
            shift
            ;;
        --no-desktop)
            ENABLE_DESKTOP="no"
            shift
            ;;
        --no-verify)
            VERIFY="no"
            shift
            ;;
        --from-archive)
            FROM_ARCHIVE="$2"
            shift 2
            ;;
        --uninstall)
            DO_UNINSTALL="yes"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            die "unknown option: $1"
            ;;
    esac
done

if [ "$INSTALL_SCOPE" = "system" ]; then
    ensure_write_dir "$PREFIX"
fi

run_uninstall

OS="$(uname -s)"
ARCH="$(uname -m)"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

script_dir="$(resolve_script_dir)"
if [ -z "$FROM_ARCHIVE" ] && { [ -f "$script_dir/teletipo" ] || [ -d "$script_dir/Teletipo.app" ]; }; then
    FROM_ARCHIVE="$script_dir"
fi

work_dir="$tmp_dir/work"
mkdir -p "$work_dir"

if [ -n "$FROM_ARCHIVE" ]; then
    log "using local archive contents from ${FROM_ARCHIVE}"
    cp -R "$FROM_ARCHIVE"/. "$work_dir" 2>/dev/null || true
else
    if [ "$VERSION" = "latest" ]; then
        VERSION="$(get_latest_tag)"
    fi

    case "$OS" in
        Linux)
            asset="teletipo-linux-x86_64.tar.gz"
            ;;
        Darwin)
            if [ "$ENABLE_DESKTOP" = "yes" ]; then
                asset="teletipo-macos-app.tar.gz"
            else
                asset="teletipo-macos-universal.tar.gz"
            fi
            ;;
        *)
            die "unsupported OS: ${OS}"
            ;;
    esac

    case "$ARCH" in
        x86_64|amd64|arm64|aarch64)
            :
            ;;
        *)
            die "unsupported architecture: ${ARCH}"
            ;;
    esac

    archive_path="${tmp_dir}/${asset}"
    download_to "${BASE_RELEASE_URL}/download/${VERSION}/${asset}" "$archive_path"

    if [ "$VERIFY" = "yes" ]; then
        sums_path="${tmp_dir}/SHA256SUMS"
        download_to "${BASE_RELEASE_URL}/download/${VERSION}/SHA256SUMS" "$sums_path"
        verify_sha256 "$archive_path" "$sums_path" "$asset"
    fi

    extract_archive "$archive_path" "$work_dir"
fi

if [ "$OS" = "Linux" ]; then
    install_linux "$work_dir"
elif [ "$OS" = "Darwin" ]; then
    if [ "$ENABLE_DESKTOP" = "yes" ]; then
        install_macos_app "$work_dir"
    elif [ "$ENABLE_DESKTOP" = "auto" ] && { [ -d "$work_dir/Teletipo.app" ] || [ -d "$work_dir/teletipo-macos-app/Teletipo.app" ]; }; then
        install_macos_app "$work_dir"
    else
        install_macos_cli "$work_dir"
    fi
fi

log "done"
