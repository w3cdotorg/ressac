#!/usr/bin/env bash
# Proves a Ressac RPM installs, starts and uninstalls cleanly on a bare Fedora.
#   apps/linux/packaging/verify-rpm.sh RPM SRCDIR
# Run as root in a fresh fedora:44 container; SRCDIR is a (read-only) copy of the repository, for
# the contract fixture and the licence file list. RPM is named Ressac-<version>-linux-<arch>.rpm.
# Prints "verify: <check> ok" for each of the seven checks and exits 0 only if all pass.
set -euo pipefail

usage='usage: apps/linux/packaging/verify-rpm.sh RPM SRCDIR'
[ "$#" -eq 2 ] || {
  echo "$usage" >&2
  exit 2
}
rpm_file=$1
srcdir=$2

fail() {
  echo "verify: $1 FAILED: $2" >&2
  exit 1
}
ok() {
  echo "verify: $1 ok"
}

[ "$(id -u)" -eq 0 ] || fail setup "run me as root (in a throwaway container)"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# 1. The RPM installs, and dnf pulls every dependency it declares.
dnf install -y "$rpm_file" >"$work/install.log" 2>&1 || {
  cat "$work/install.log" >&2
  fail install "dnf install $rpm_file"
}
ok install

# The checks' own tools (not the app's), with no weak dependencies: runuser is in util-linux.
dnf install -y --setopt=install_weak_deps=False desktop-file-utils util-linux >"$work/tools.log" 2>&1 || {
  cat "$work/tools.log" >&2
  fail setup "dnf install of the checks' tools"
}

# 2. The package holds exactly the two binaries, the desktop entry, the icon and the licences.
{
  printf '%s\n' \
    /usr/bin/ressac \
    /usr/bin/ressac-kde \
    /usr/share/applications/org.w3cdotorg.ressac.desktop \
    /usr/share/icons/hicolor/scalable/apps/org.w3cdotorg.ressac.svg \
    /usr/share/licenses/ressac
  for licence in \
    "$srcdir"/apps/macos/Ressac/Resources/Fonts/*-OFL.txt \
    "$srcdir"/apps/macos/Ressac/Resources/Fonts/Bedstead-CC0.txt \
    "$srcdir"/apps/macos/Ressac/Resources/Chart/LICENSE-d3.txt \
    "$srcdir"/apps/macos/Ressac/Resources/Chart/fonts/*-OFL.txt; do
    [ -f "$licence" ] || fail files "missing licence source $licence"
    echo "/usr/share/licenses/ressac/${licence##*/}"
  done
} | sort >"$work/expected"
rpm -ql ressac | sort >"$work/actual"
diff -u "$work/expected" "$work/actual" >&2 || fail files "rpm -ql ressac differs from the expected list (above)"
ok files

# 3. The terminal binary reports the version the file name carries, and so does the package.
name=${rpm_file##*/}
[[ "$name" =~ ^Ressac-(.+)-linux-(x86_64|aarch64)\.rpm$ ]] || fail version "unexpected file name $name"
version=${BASH_REMATCH[1]}
printed=$(ressac --version) || fail version "ressac --version exited non-zero"
[ "$printed" = "ressac $version" ] || fail version "ressac --version printed '$printed', want 'ressac $version'"
package_version=$(rpm -q --qf '%{VERSION}' ressac)
[ "$package_version" = "$(printf '%s' "$version" | tr - '~')" ] ||
  fail version "package Version $package_version does not match $version"
ok version

# 4. Every library ressac-kde links to is on the system.
missing=$(ldd /usr/bin/ressac-kde | grep 'not found' || true)
[ -z "$missing" ] || fail libraries "$missing"
ok libraries

# 5. The desktop entry is valid.
desktop-file-validate /usr/share/applications/org.w3cdotorg.ressac.desktop || fail desktop "desktop-file-validate"
ok desktop

# 6. A non-root user starts the app offscreen on a copy of the contract fixture: the launch
# screen loads the reading page (Kirigami, the Breeze controls, QtWebEngine), the `mark-all`
# walk (the shortest one on a library with feeds) runs, and QML reports no error. The reader's
# proof line (ReaderPane.qml) is logged only once a web page has loaded and run its script, so
# it proves QtWebEngine and its helper process work.
! id verify >/dev/null 2>&1 || fail setup "a user 'verify' already exists: run me in a fresh container"
useradd --create-home --user-group verify
home=/home/verify
db=$home/.local/share/ressac/ressac.sqlite
runuser -u verify -- mkdir -p "${db%/*}" "$home/shots"
runuser -u verify -- cp "$srcdir/tests/fixtures/contract.sqlite" "$db"
status=0
runuser -u verify -- env -i -C "$home" \
  HOME="$home" USER=verify LOGNAME=verify PATH=/usr/bin:/bin TZ=UTC \
  XDG_CONFIG_HOME="$home/.config" XDG_DATA_HOME="$home/.local/share" \
  XDG_STATE_HOME="$home/.local/state" XDG_CACHE_HOME="$home/.cache" \
  QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
  RESSAC_DB="$db" RESSAC_RECETTE_WALK=mark-all \
  timeout 120 ressac-kde --screenshot "$home/shots" >"$work/app.log" 2>&1 || status=$?
if [ "$status" -ne 0 ]; then
  tail -n 60 "$work/app.log" >&2
  fail launch "ressac-kde exited $status"
fi
for bad in "failed to load component" "is not installed" "ReferenceError" "TypeError" "Unable to assign"; do
  if grep -qF "$bad" "$work/app.log"; then
    grep -F "$bad" "$work/app.log" >&2
    fail launch "QML error: $bad"
  fi
done
for line in 'reader-proof: {"app":"ran"' "walk: 02-journal rows=4 can=true" "walk: 03-mark-all can=false"; do
  grep -qF "$line" "$work/app.log" || {
    tail -n 60 "$work/app.log" >&2
    fail launch "missing log line: $line"
  }
done
[ -s "$home/shots/01-ready.png" ] || fail launch "no $home/shots/01-ready.png (launch screen not ready)"
ok launch

# 7. Removing the package removes its files and leaves the user's home alone.
(cd "$home" && find . -type f -print0 | sort -z | xargs -0 sha256sum) >"$work/home.before"
dnf remove -y ressac >"$work/remove.log" 2>&1 || {
  cat "$work/remove.log" >&2
  fail remove "dnf remove ressac"
}
! rpm -q --quiet ressac || fail remove "ressac is still installed"
while read -r file; do
  [ ! -e "$file" ] || fail remove "$file is still there"
done <"$work/expected"
[ -s "$db" ] || fail remove "the database copy $db is gone"
(cd "$home" && find . -type f -print0 | sort -z | xargs -0 sha256sum) >"$work/home.after"
diff -u "$work/home.before" "$work/home.after" >&2 || fail remove "the home folder changed"
ok remove
