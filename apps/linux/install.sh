#!/usr/bin/env bash
# Installs Ressac for the current user on Fedora KDE: the `ressac` CLI and the `ressac-kde` app in
# ~/.local/bin, the icon and a generated desktop entry under $XDG_DATA_HOME (else ~/.local/share).
#   apps/linux/install.sh [--software-rendering]   check packages, build in release mode, install
#   apps/linux/install.sh --uninstall              remove exactly those files, nothing else
# The database, the settings (ressac-kde.conf), the logs and the caches are never touched.
set -euo pipefail

usage='usage: apps/linux/install.sh [--software-rendering] | --uninstall'

die() {
  echo "install.sh: $*" >&2
  exit 2
}

software=false
uninstall=false
case "${1:-}:$#" in
  :0) ;;
  --software-rendering:1) software=true ;;
  --uninstall:1) uninstall=true ;;
  *)
    echo "$usage" >&2
    exit 2
    ;;
esac

[ "$(uname -s)" = Linux ] || die "run me on Linux (Fedora)"
[ "$(id -u)" -ne 0 ] || die "run me as your user, not root"
case "${HOME:-}" in
  /*) ;;
  *) die "HOME is not an absolute path" ;;
esac

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$script_dir/../.." && pwd -P)

bindir="$HOME/.local/bin"
case "${XDG_DATA_HOME:-}" in
  /*) data="$XDG_DATA_HOME" ;;
  *) data="$HOME/.local/share" ;;
esac
icon="$data/icons/hicolor/scalable/apps/org.w3cdotorg.ressac.svg"
desktop="$data/applications/org.w3cdotorg.ressac.desktop"
installed_files=("$bindir/ressac" "$bindir/ressac-kde" "$icon" "$desktop")

for dir in "$bindir" "$data"; do
  case "$dir" in
    *[\"\`\$\\%]*) die "$dir holds a character a .desktop Exec line cannot carry" ;;
  esac
done

if $uninstall; then
  for file in "${installed_files[@]}"; do
    if [ -e "$file" ] || [ -L "$file" ]; then
      rm -f "$file"
      echo "removed $file"
    fi
  done
  echo "install.sh: uninstalled (database, settings and logs kept)"
  exit 0
fi

# Packages: every one of the dev container's list that the app needs on a desktop. The script
# never runs dnf itself.
container_only=" rustfmt clippy nodejs cmake shadow-utils util-linux-core procps-ng findutils diffutils tar gzip git-core "
[ -r "$script_dir/dev/packages.txt" ] || die "cannot read $script_dir/dev/packages.txt"
missing=()
while read -r package; do
  [ -n "$package" ] || continue
  case "$container_only" in
    *" $package "*) continue ;;
  esac
  rpm -q --quiet "$package" || missing+=("$package")
done < <(sed 's/#.*//' "$script_dir/dev/packages.txt" | xargs -n1)
if [ "${#missing[@]}" -gt 0 ]; then
  echo "install.sh: missing Fedora packages; install them with:" >&2
  echo "  sudo dnf install ${missing[*]}" >&2
  exit 1
fi

cd "$repo"
cargo build --release --locked -p ressac -p ressac-kde
release="${CARGO_TARGET_DIR:-$repo/target}/release"

install -Dm755 "$release/ressac" "$bindir/ressac"
echo "installed $bindir/ressac"
install -Dm755 "$release/ressac-kde" "$bindir/ressac-kde"
echo "installed $bindir/ressac-kde"
install -Dm644 "$repo/apps/macos/Ressac/Resources/AppIcon.svg" "$icon"
echo "installed $icon"

exec_line="\"$bindir/ressac-kde\""
if $software; then
  exec_line="env QT_QUICK_BACKEND=software QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu $exec_line"
fi
desktop_tmp=$(mktemp)
trap 'rm -f "$desktop_tmp"' EXIT
cat >"$desktop_tmp" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Ressac
GenericName=Feed Reader
Comment=Tidy your RSS subscriptions, then read them calmly
Exec=$exec_line
Icon=org.w3cdotorg.ressac
Terminal=false
Categories=Network;Feed;News;Qt;KDE;
StartupWMClass=ressac-kde
DESKTOP
install -Dm644 "$desktop_tmp" "$desktop"
echo "installed $desktop"

case ":$PATH:" in
  *":$bindir:"*) ;;
  *) echo "install.sh: $bindir is not on PATH; add it to use ressac in Konsole." ;;
esac
echo "install.sh: installed ressac and ressac-kde in $bindir"
