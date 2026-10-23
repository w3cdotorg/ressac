#!/usr/bin/env bash
# The Linux gate: fmt, clippy, tests (hermetic), chart tests, XDG layout smoke
# test. Run it through apps/linux/dev/dev.sh check.
set -euo pipefail

die() {
  echo "check-linux: $*" >&2
  exit 2
}

[ "$(uname -s)" = Linux ] || die "run me on Linux (apps/linux/dev/dev.sh check)"
[ "$(id -u)" -ne 0 ] || die "run me as a non-root user (the Chromium sandbox and the store tests need it)"

cd "$(dirname "$0")/.."
repo=$(pwd -P)

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/target}"
target=$(realpath -m "$CARGO_TARGET_DIR")
case "$target/" in
  "$repo"/*) die "CARGO_TARGET_DIR is inside the repository: $CARGO_TARGET_DIR" ;;
esac
export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}" QT_FORCE_STDERR_LOGGING=1

cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/home"
env -u RESSAC_DB -u RUST_LOG \
  HOME="$scratch/home" \
  XDG_DATA_HOME="$scratch/xdg/data" \
  XDG_CONFIG_HOME="$scratch/xdg/config" \
  XDG_STATE_HOME="$scratch/xdg/state" \
  XDG_CACHE_HOME="$scratch/xdg/cache" \
  cargo test --locked --workspace
leftover=$(find "$scratch/home" "$scratch/xdg" -mindepth 1 2>/dev/null | head -n 20 || true)
[ -z "$leftover" ] || die "tests wrote outside their temp dirs:
$leftover"

# Layout smoke test: a fresh HOME and no XDG_* must give the XDG layout.
smoke="$scratch/smoke"
mkdir "$smoke"
env -u RESSAC_DB -u RUST_LOG -u XDG_DATA_HOME -u XDG_CONFIG_HOME -u XDG_STATE_HOME -u XDG_CACHE_HOME \
  HOME="$smoke" "$target/debug/ressac" import tests/fixtures/vienna-like.opml
test -f "$smoke/.local/share/ressac/ressac.sqlite" \
  || die "layout smoke: $smoke/.local/share/ressac/ressac.sqlite is missing"
test -d "$smoke/.local/state/ressac/logs" \
  || die "layout smoke: $smoke/.local/state/ressac/logs is missing"
test ! -e "$smoke/Library" || die "layout smoke: $smoke/Library exists"

node --test 'apps/macos/ChartTests/*.test.cjs'

echo "check-linux: OK (fmt, clippy, test, chart, layout smoke)"
