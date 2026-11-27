#!/usr/bin/env bash
# Builds the Ressac RPM (the KDE app and the terminal app) for this machine's architecture.
#   apps/linux/packaging/build-rpm.sh VERSION OUTDIR
# Run from the repository root on Fedora 44 with apps/linux/dev/packages.txt plus rpm-build and
# desktop-file-utils installed, as a regular user. Compiles both binaries in release mode, packages
# them with ressac.spec and writes OUTDIR/Ressac-VERSION-linux-<arch>.rpm, whose path it prints
# last. A pre-release VERSION (0.4.0-beta.1) becomes the RPM Version 0.4.0~beta.1, which sorts
# before 0.4.0; the file name keeps VERSION as given.
set -euo pipefail

usage='usage: apps/linux/packaging/build-rpm.sh VERSION OUTDIR'

die() {
  echo "build-rpm.sh: $*" >&2
  exit 2
}

[ "$#" -eq 2 ] || {
  echo "$usage" >&2
  exit 2
}
version=$1
outdir=$2

[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || die "VERSION must look like 1.2.3 or 1.2.3-beta.1, not '$version'"
spec=apps/linux/packaging/ressac.spec
if [ ! -f "$spec" ] || [ ! -f Cargo.lock ]; then
  die "run me from the repository root"
fi

# The binaries report the workspace version: VERSION must be it, or a pre-release of it.
workspace=$(awk -F'"' '/^\[workspace.package\]/ { p = 1 } p && /^version *=/ { print $2; exit }' Cargo.toml)
[ "${version%%-*}" = "${workspace%%-*}" ] ||
  die "VERSION $version does not match the workspace version $workspace in Cargo.toml"

command -v rpmbuild >/dev/null || die "rpmbuild is missing (dnf install rpm-build)"

rpm_version=$(printf '%s' "$version" | tr - '~')
arch=$(uname -m)
srcdir=$PWD

cargo build --release --locked -p ressac -p ressac-kde
target=${CARGO_TARGET_DIR:-target}
case "$target" in
  /*) ;;
  *) target="$srcdir/$target" ;;
esac
bindir="$target/release"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
rpmbuild -bb \
  --define "_topdir $tmp" \
  --define "ressac_version $rpm_version" \
  --define "ressac_srcdir $srcdir" \
  --define "ressac_bindir $bindir" \
  "$spec"

shopt -s nullglob
built=("$tmp"/RPMS/"$arch"/ressac-"$rpm_version"-*."$arch".rpm)
[ "${#built[@]}" -eq 1 ] || die "expected one RPM under $tmp/RPMS/$arch, found ${#built[@]}"

mkdir -p "$outdir"
rpm_file="$outdir/Ressac-$version-linux-$arch.rpm"
cp "${built[0]}" "$rpm_file"
echo "$rpm_file"
