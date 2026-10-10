#!/bin/sh
# Builds the ressac CLI and embeds it as Ressac.app/Contents/Helpers/ressac (spec §4, Bundling).
# Run by Xcode as a build phase; SRCROOT is apps/macos.
set -eu
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
repo_root="$SRCROOT/../.."
helpers="$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH/Helpers"
# Xcode exports SDK and toolchain variables that are meant for Swift, not for cargo.
env -u SDKROOT -u MACOSX_DEPLOYMENT_TARGET -u ARCHS -u LD -u CC -u CXX \
  cargo build --release -p ressac --manifest-path "$repo_root/Cargo.toml"
mkdir -p "$helpers"
cp "$repo_root/target/release/ressac" "$helpers/ressac"
codesign --force --sign - --timestamp=none "$helpers/ressac"
