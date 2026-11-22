#!/usr/bin/env bash
# computer-use local installer.
# Default mode installs an already-built portable release:
#   scripts/install-local.sh [--prefix ~/.local] [archive | bundle | binary]
# The legacy developer flags below select the retained Rust backend installer,
# which builds and installs `cua-driver-local` separately from released copies.
#
# Flags (forwarded verbatim to the Rust helper):
#   --release              build the release configuration (default: debug)
#   --autostart            register an auto-start daemon (macOS: LaunchAgent;
#                          Linux: systemd user unit). Default off.
#   --require-stable-signing
#                          on macOS, refuse to install an ad-hoc-signed app
#   --bin-dir <path>       override the symlink destination (default ~/.local/bin)
#   --backend=rust         explicit Rust backend (no-op; Rust is the only option)
#   --experimental-rust    legacy alias for --backend=rust (no-op)
#   --backend=swift        retired Swift backend (no-op; accepted for compat)
#
# Other upstream download installers still target upstream artifact names.
set -euo pipefail
legacy=0
for arg in "$@"; do
    case "$arg" in
        --release|--autostart|--require-stable-signing|--bin-dir|--bin-dir=*|--backend=*|--experimental-rust|--)
            legacy=1 ;;
    esac
done
if [[ "$legacy" == 0 ]]; then
    exec "$(dirname -- "${BASH_SOURCE[0]}")/_install-portable.sh" "$@"
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# --- Lightweight flag parsing -----------------------------------------------
# Consume backend flags (no-ops for compat) and forward the rest to the Rust helper.
FORWARDED_ARGS=()
PASSTHROUGH=0
while [[ $# -gt 0 ]]; do
    if [[ "$PASSTHROUGH" == "1" ]]; then
        FORWARDED_ARGS+=("$1"); shift; continue
    fi
    case "$1" in
        --backend=rust)      shift ;;                 # explicit Rust (no-op)
        --experimental-rust) shift ;;                 # legacy alias (no-op)
        --backend=swift)     shift ;;                 # retired Swift (no-op)
        --backend=*)
            printf 'error: unknown backend %q; supported: rust\n' "${1#*=}" >&2
            exit 2
            ;;
        --)                  PASSTHROUGH=1; shift ;;  # forward the rest verbatim
        *)                   FORWARDED_ARGS+=("$1"); shift ;;
    esac
done

HELPER="$SCRIPT_DIR/_install-local-rust.sh"
if [[ ! -f "$HELPER" ]]; then
    printf 'error: backend helper not found at %s\n' "$HELPER" >&2
    exit 1
fi

# `${arr[@]+"${arr[@]}"}` guards `set -u` on the zero-arg case (macOS bash 3.2).
exec /bin/bash "$HELPER" ${FORWARDED_ARGS[@]+"${FORWARDED_ARGS[@]}"}
