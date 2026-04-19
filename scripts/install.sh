#!/bin/sh
# synrepo install script
# https://github.com/whit3rabbit/synrepo
#
# Behavior:
#   macOS: if `brew` is on PATH, installs via `brew install --cask whit3rabbit/tap/synrepo`.
#          Set SYNREPO_SKIP_BREW=1 to force a direct binary install instead.
#   Linux and macOS-fallback: installs the verified release binary to
#   ${SYNREPO_INSTALL_DIR:-$HOME/.local/bin}. If that directory is not on PATH,
#   a single guarded block is appended to the user's shell rc file
#   (.zshrc / .bashrc / .profile) so new shells pick it up.
#
# All downloads are verified against the release SHA256SUMS file before install.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.sh | sh
#   INSTALL_VERSION=0.0.1 sh install.sh
#   sh install.sh --version 0.0.1

set -eu

REPO="whit3rabbit/synrepo"
BREW_TARGET="whit3rabbit/tap/synrepo"
MARK_BEGIN="# >>> synrepo install >>>"
MARK_END="# <<< synrepo install <<<"

cleanup() {
    if [ -n "${_TMPDIR:-}" ] && [ -d "${_TMPDIR}" ]; then
        rm -rf "${_TMPDIR}"
    fi
}

die() {
    printf '%s\n' "$1" >&2
    exit 1
}

info() {
    printf '==> %s\n' "$1"
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "error: required command not found: $1"
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

# Extract the checksum for a given filename from a SHA256SUMS file.
# Tolerates both "<sha>  name" and "<sha>  ./name" entries.
fetch_checksum() {
    awk -v f="$1" '$2 == f || $2 == "./"f {print $1; exit}' "$2"
}

# Resolve the shell rc file to edit when adding install_dir to PATH.
resolve_rc_file() {
    _shell="$(basename "${SHELL:-}")"
    case "${_shell}" in
        zsh)  printf '%s\n' "${HOME}/.zshrc" ;;
        bash) printf '%s\n' "${HOME}/.bashrc" ;;
        *)    printf '%s\n' "${HOME}/.profile" ;;
    esac
}

# Append a guarded block exporting install_dir onto PATH if not already present.
ensure_path_on_shell_rc() {
    _dir="$1"
    _rc="$(resolve_rc_file)"
    [ -f "${_rc}" ] || : > "${_rc}"
    if grep -Fq "${MARK_BEGIN}" "${_rc}" 2>/dev/null; then
        info "${_rc} already contains a synrepo PATH block; leaving it alone."
        return 0
    fi
    # The literal ${PATH} in the printf strings is intentional: we are writing
    # shell code that must be evaluated later, not expanding PATH here.
    # shellcheck disable=SC2016
    {
        printf '\n%s\n' "${MARK_BEGIN}"
        printf 'case ":${PATH}:" in\n'
        printf '  *":%s:"*) ;;\n' "${_dir}"
        printf '  *) export PATH="%s:${PATH}" ;;\n' "${_dir}"
        printf 'esac\n'
        printf '%s\n' "${MARK_END}"
    } >> "${_rc}"
    info "Added ${_dir} to PATH via ${_rc}."
    printf '    Run: source "%s"  (or restart your shell)\n' "${_rc}"
}

main() {
    trap cleanup EXIT INT TERM

    # --- arg/env parsing ---
    _VERSION="${INSTALL_VERSION:-}"
    while [ $# -gt 0 ]; do
        case "$1" in
            -v|--version)
                [ $# -ge 2 ] || die "error: $1 requires an argument"
                _VERSION="$2"
                shift 2
                ;;
            -h|--help)
                cat <<'EOF'
Usage: install.sh [-v VERSION]

Environment:
  INSTALL_VERSION       Pin the version to install (e.g. 0.0.1).
  SYNREPO_INSTALL_DIR   Override install directory (default: $HOME/.local/bin).
  SYNREPO_SKIP_BREW     Set to 1 to skip Homebrew even when brew is present.
EOF
                exit 0
                ;;
            *)
                die "error: unknown argument: $1"
                ;;
        esac
    done

    need_cmd curl

    _OS=$(uname -s)
    _ARCH=$(uname -m)

    # --- macOS: prefer Homebrew when available ---
    if [ "${_OS}" = "Darwin" ] && [ -z "${SYNREPO_SKIP_BREW:-}" ] && command -v brew >/dev/null 2>&1; then
        info "Homebrew detected. Installing synrepo via brew (${BREW_TARGET})."
        # brew install --cask returns 0 even when the cask is not in any tap
        # (it only prints a warning). Verify after the fact and fall through to
        # the direct-download path if brew didn't actually install the cask.
        brew install --cask "${BREW_TARGET}" || true
        if brew list --cask synrepo >/dev/null 2>&1; then
            info "Installed synrepo via Homebrew."
            printf '    Upgrade later: brew upgrade --cask %s\n' "${BREW_TARGET}"
            synrepo --version 2>/dev/null || true
            exit 0
        fi
        info "Homebrew did not install ${BREW_TARGET}; falling back to direct download."
    fi

    # --- raw-binary path: resolve version ---
    if [ -z "${_VERSION}" ]; then
        _VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
            | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p' \
            | head -1)
        [ -n "${_VERSION}" ] || die "error: could not determine latest version from GitHub"
    fi

    # --- target selection ---
    case "${_OS}:${_ARCH}" in
        Darwin:arm64)          _TARGET="macos-arm64" ;;
        Darwin:x86_64)         _TARGET="macos-x86_64" ;;
        Linux:x86_64)          _TARGET="linux-amd64" ;;
        Linux:aarch64|Linux:arm64) _TARGET="linux-arm64" ;;
        *) die "error: unsupported platform: ${_OS} ${_ARCH}" ;;
    esac

    _BASE_URL="https://github.com/${REPO}/releases/download/v${_VERSION}"
    _BINARY_NAME="synrepo-${_VERSION}-${_TARGET}"

    _TMPDIR=$(mktemp -d)

    info "Downloading ${_BINARY_NAME}..."
    curl -fsSL "${_BASE_URL}/${_BINARY_NAME}" -o "${_TMPDIR}/synrepo" \
        || die "error: failed to download ${_BINARY_NAME}"
    curl -fsSL "${_BASE_URL}/SHA256SUMS" -o "${_TMPDIR}/SHA256SUMS" \
        || die "error: failed to download SHA256SUMS"

    info "Verifying checksum..."
    _EXPECTED=$(fetch_checksum "${_BINARY_NAME}" "${_TMPDIR}/SHA256SUMS")
    [ -n "${_EXPECTED}" ] || die "error: ${_BINARY_NAME} not found in SHA256SUMS"
    _ACTUAL=$(compute_sha256 "${_TMPDIR}/synrepo")
    if [ "${_EXPECTED}" != "${_ACTUAL}" ]; then
        die "error: checksum mismatch for ${_BINARY_NAME}
  expected: ${_EXPECTED}
  actual:   ${_ACTUAL}"
    fi

    # --- install ---
    _DEST_DIR="${SYNREPO_INSTALL_DIR:-${HOME}/.local/bin}"
    mkdir -p "${_DEST_DIR}"
    cp "${_TMPDIR}/synrepo" "${_DEST_DIR}/synrepo"
    chmod 755 "${_DEST_DIR}/synrepo"
    info "Installed synrepo ${_VERSION} to ${_DEST_DIR}/synrepo"

    # --- PATH handling ---
    case ":${PATH}:" in
        *":${_DEST_DIR}:"*)
            "${_DEST_DIR}/synrepo" --version 2>/dev/null || true
            ;;
        *)
            ensure_path_on_shell_rc "${_DEST_DIR}"
            # Run the binary once via absolute path so the user sees it works.
            "${_DEST_DIR}/synrepo" --version 2>/dev/null || true
            ;;
    esac
}

main "$@"
