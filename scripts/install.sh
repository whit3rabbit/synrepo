#!/bin/sh
# synrepo install script
# https://github.com/whit3rabbit/synrepo
set -eu

REPO="whit3rabbit/synrepo"

cleanup() {
    if [ -n "${_TMPDIR:-}" ] && [ -d "${_TMPDIR}" ]; then
        rm -rf "${_TMPDIR}"
    fi
}

die() {
    printf '%s\n' "$1" >&2
    exit 1
}

compute_sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        die "error: sha256sum or shasum required but not found"
    fi
}

main() {
    trap cleanup EXIT INT TERM

    _TMPDIR=$(mktemp -d)

    # --- version resolution ---
    _VERSION="${INSTALL_VERSION:-}"
    if [ $# -gt 0 ]; then
        while [ $# -gt 0 ]; do
            case "$1" in
                -v|--version)
                    [ $# -ge 2 ] || die "error: $1 requires an argument"
                    _VERSION="$2"
                    shift 2
                    ;;
                -h|--help)
                    printf 'Usage: install.sh [-v VERSION]\n'
                    printf '  INSTALL_VERSION env var also accepted.\n'
                    exit 0
                    ;;
                *)
                    die "error: unknown argument: $1"
                    ;;
            esac
        done
    fi

    if [ -z "${_VERSION}" ]; then
        _VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
            | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p' \
            | head -1)
        [ -n "${_VERSION}" ] || die "error: could not determine latest version from GitHub"
    fi

    # --- OS and architecture detection ---
    _OS=$(uname -s)
    _ARCH=$(uname -m)
    _TARGET=""

    case "${_OS}:${_ARCH}" in
        Darwin:arm64)  _TARGET="macos-arm64" ;;
        Darwin:x86_64) _TARGET="macos-x86_64" ;;
        Linux:x86_64)  _TARGET="linux-amd64" ;;
        Linux:aarch64) _TARGET="linux-arm64" ;;
        *) die "error: unsupported platform: ${_OS} ${_ARCH}" ;;
    esac

    if [ "${_OS}" = "Darwin" ]; then
        printf 'note: on macOS, brew install whit3rabbit/tap/synrepo is the preferred install method\n'
    fi

    # --- download ---
    _BASE_URL="https://github.com/${REPO}/releases/download/v${_VERSION}"
    _BINARY_NAME="synrepo-${_VERSION}-${_TARGET}"

    printf 'downloading synrepo %s for %s...\n' "${_VERSION}" "${_TARGET}"
    curl -fsSL "${_BASE_URL}/${_BINARY_NAME}" -o "${_TMPDIR}/synrepo" \
        || die "error: failed to download ${_BINARY_NAME}"
    curl -fsSL "${_BASE_URL}/SHA256SUMS" -o "${_TMPDIR}/SHA256SUMS" \
        || die "error: failed to download SHA256SUMS"

    # --- checksum verification ---
    _EXPECTED=$(grep "  ${_BINARY_NAME}\$" "${_TMPDIR}/SHA256SUMS" | cut -d' ' -f1)
    [ -n "${_EXPECTED}" ] || die "error: ${_BINARY_NAME} not found in SHA256SUMS"
    _ACTUAL=$(compute_sha256 "${_TMPDIR}/synrepo")

    if [ "${_EXPECTED}" != "${_ACTUAL}" ]; then
        die "error: checksum mismatch for ${_BINARY_NAME}
  expected: ${_EXPECTED}
  actual:   ${_ACTUAL}"
    fi

    # --- install ---
    if [ -n "${SYNREPO_INSTALL_DIR:-}" ]; then
        _DEST_DIR="${SYNREPO_INSTALL_DIR}"
    elif [ -w "/usr/local/bin" ]; then
        _DEST_DIR="/usr/local/bin"
    else
        _DEST_DIR="${HOME}/.local/bin"
        mkdir -p "${_DEST_DIR}"
    fi

    cp "${_TMPDIR}/synrepo" "${_DEST_DIR}/synrepo"
    chmod 755 "${_DEST_DIR}/synrepo"

    printf 'installed synrepo %s to %s/synrepo\n' "${_VERSION}" "${_DEST_DIR}"

    # --- PATH check ---
    case ":${PATH}:" in
        *":${_DEST_DIR}:"*) ;;
        *) printf 'warning: %s is not on your PATH\n' "${_DEST_DIR}" ;;
    esac
}

main "$@"
