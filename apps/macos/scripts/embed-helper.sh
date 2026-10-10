#!/bin/sh
# Builds the ressac CLI and embeds it as Ressac.app/Contents/Helpers/ressac (spec §4, Bundling).
# Run by Xcode as a build phase; SRCROOT is apps/macos. Honours CARGO_TARGET_DIR.
set -eu
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found (looked in /opt/homebrew/bin and ~/.cargo/bin). Install Rust from https://rustup.rs" >&2
  exit 1
fi
repo_root="$SRCROOT/../.."
helpers="$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH/Helpers"
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
# Xcode's toolchain clang has no default sysroot: give cargo's C builds (libsqlite3-sys,
# aws-lc-sys) a valid macOS SDK, and drop the Swift-only variables.
sdk="$(xcrun --sdk macosx --show-sdk-path)"
env -u MACOSX_DEPLOYMENT_TARGET -u ARCHS -u LD -u CC -u CXX \
  SDKROOT="$sdk" CARGO_TARGET_DIR="$target_dir" \
  cargo build --release -p ressac --manifest-path "$repo_root/Cargo.toml"
mkdir -p "$helpers"
cp "$target_dir/release/ressac" "$helpers/ressac"
codesign --force --sign - --timestamp=none "$helpers/ressac"
