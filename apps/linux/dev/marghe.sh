#!/usr/bin/env bash
# Drives Clément's Fedora KDE VM (Parallels "Fedora Linux" on his Mac marghe) from this Mac:
# transfer the committed tree, build and install it there, run the screenshot walk inside his live
# Plasma session, hand-check with `manual`. Used for the VM checkpoints of sub-project 8.
#   apps/linux/dev/marghe.sh check
#   apps/linux/dev/marghe.sh push
#   apps/linux/dev/marghe.sh install [--software-rendering]
#   RESSAC_VM_DNF_OK=1 apps/linux/dev/marghe.sh dnf <package...>
#   apps/linux/dev/marghe.sh shots <label> [--software-rendering] [--font <name>] [--look antiope|native]
#       [--walk NAME] [--big N [--long]] [--env K=V]...
#   apps/linux/dev/marghe.sh manual [--look antiope|native] [--big N [--long]] [--frame-log] | manual stop
# `install` builds in release mode (apps/linux/install.sh), and `shots` and `manual` run that
# installed binary (~/.local/bin/ressac-kde): their frame times are release frame times.
# `--big N` builds a copy of the test fixture on this Mac with N more entries in xkcd (feed 1) and
# uploads it as the run's database; `--long` gives the first 200 of them a long article (about
# 30 kB, as the container's `scrolling_two_thousand_entries_is_logged`).
# Everything on the VM stays under ~/src/ressac, ~/.cache/ressac-build, ~/.local and /tmp/ressac-vm.
# shellcheck disable=SC2029,SC2016
set -euo pipefail

HOST=willow@marghe.local
VM="Fedora Linux"
VM_USER=willow
usage='usage: marghe.sh check | push | install [--software-rendering] | dnf <package...>
  | shots <label> [--software-rendering] [--font systemSans|systemSerif|newsreader|atkinson] [--look antiope|native]
      [--walk NAME] [--big N [--long]] [--env K=V]...
  | manual [--look antiope|native] [--big N [--long]] [--frame-log] | manual stop'

die() {
  echo "marghe.sh: $*" >&2
  exit 1
}

bad_usage() {
  echo "$usage" >&2
  exit 2
}

# ---------------------------------------------------------------------------------------------
# VM side: `marghe.sh --in-vm shots|manual ...`, run as the VM user from the pushed tree.
# ---------------------------------------------------------------------------------------------
session_env() {
  pid=$(pgrep -u "$VM_USER" -x plasmashell | head -1 || true)
  if [ -z "$pid" ]; then
    echo "Clément is not logged in to Plasma on the VM" >&2
    exit 3
  fi
  for name in WAYLAND_DISPLAY XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS XDG_SESSION_TYPE \
    XDG_CURRENT_DESKTOP QT_QPA_PLATFORMTHEME KDE_FULL_SESSION KDE_SESSION_VERSION \
    XDG_SESSION_DESKTOP DESKTOP_SESSION DISPLAY; do
    value=$(tr '\0' '\n' <"/proc/$pid/environ" | sed -n "s/^$name=//p" | head -1 || true)
    if [ -n "$value" ]; then
      export "$name=$value"
    fi
  done
}

# write_conf FONT LOOK: a fresh settings file holding only those keys (none when both are empty).
write_conf() {
  local lines=()
  [ -z "$1" ] || lines+=("reader.font=$1")
  [ -z "$2" ] || lines+=("appearance=$2")
  [ "${#lines[@]}" -gt 0 ] || return 0
  mkdir -p "$(dirname "$conf")"
  {
    echo '[General]'
    printf '%s\n' "${lines[@]}"
  } >"$conf"
}

binary_line() {
  echo "binary: $bin (release build of $(cat "$HOME/.cache/ressac-build/installed-sha" 2>/dev/null || echo 'an unknown commit'))"
}

in_vm() {
  mode=$1
  shift
  repo=$HOME/src/ressac
  bin="$HOME/.local/bin/ressac-kde"
  conf=$HOME/.config/ressac/ressac-kde.conf
  case "$mode" in
    shots)
      label=$1
      shift
      software=false
      font=
      look=
      big=false
      extra=()
      while [ $# -gt 0 ]; do
        case "$1" in
          --software-rendering) software=true ;;
          --font)
            font=$2
            shift
            ;;
          --look)
            look=$2
            shift
            ;;
          --walk)
            extra+=("RESSAC_RECETTE_WALK=$2")
            shift
            ;;
          --env)
            extra+=("$2")
            shift
            ;;
          --big)
            big=true
            shift
            ;;
          --long) ;;
          *) bad_usage ;;
        esac
        shift
      done
      session_env
      binary_line
      run=/tmp/ressac-vm/$label
      if $big; then
        [ -f "$run/r.sqlite" ] || die "no uploaded database at $run/r.sqlite"
        mv "$run/r.sqlite" "/tmp/ressac-vm/.$label.sqlite"
      fi
      rm -rf "$run"
      mkdir -p "$run/shots"
      if $big; then
        mv "/tmp/ressac-vm/.$label.sqlite" "$run/r.sqlite"
      else
        cp "$repo/tests/fixtures/contract.sqlite" "$run/r.sqlite"
      fi
      saved=$run/saved.conf
      had_conf=false
      if [ -f "$conf" ]; then
        had_conf=true
        cp "$conf" "$saved"
        echo "settings file saved"
      else
        echo "settings file absent"
      fi
      write_conf "$font" "$look"
      if $software; then
        extra+=(QT_QUICK_BACKEND=software QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu)
      fi
      set +e
      env RESSAC_DB="$run/r.sqlite" QT_FORCE_STDERR_LOGGING=1 "${extra[@]}" \
        timeout 120 "$bin" --screenshot "$run/shots" >"$run/run.log" 2>&1
      code=$?
      set -e
      echo "exit=$code" >>"$run/run.log"
      if $had_conf; then
        cp "$saved" "$conf"
      else
        rm -f "$conf"
      fi
      rm -f "$saved" "$run/r.sqlite"
      echo "exit=$code"
      exit "$code"
      ;;
    manual)
      run=/tmp/ressac-vm/manual
      # The settings file `--look` replaced, kept outside the run until `manual stop`.
      keep=/tmp/ressac-vm/manual-settings
      if [ "${1:-}" = stop ]; then
        pkill -u "$VM_USER" -x ressac-kde || true
        for _ in $(seq 1 20); do
          pgrep -u "$VM_USER" -x ressac-kde >/dev/null || break
          sleep 0.5
        done
        if [ -f "$keep/saved.conf" ]; then
          cp "$keep/saved.conf" "$conf"
          echo "settings file restored"
        elif [ -f "$keep/absent" ]; then
          rm -f "$conf"
          echo "settings file removed (there was none)"
        fi
        rm -rf "$keep"
        if [ -f "$run/run.log" ]; then
          grep 'ressac-kde: frames' "$run/run.log" || echo "no frame-log line in run.log"
        fi
        rm -rf "$run"
        echo "manual run stopped"
        exit 0
      fi
      look=
      big=false
      frame_log=0
      while [ $# -gt 0 ]; do
        case "$1" in
          --look)
            look=$2
            shift
            ;;
          --big)
            big=true
            shift
            ;;
          --long) ;;
          --frame-log) frame_log=1 ;;
          *) bad_usage ;;
        esac
        shift
      done
      session_env
      if pgrep -u "$VM_USER" -x ressac-kde >/dev/null; then
        die "ressac-kde already runs in the session; run 'marghe.sh manual stop' first"
      fi
      binary_line
      if $big; then
        [ -f "$run/r.sqlite" ] || die "no uploaded database at $run/r.sqlite"
        mv "$run/r.sqlite" /tmp/ressac-vm/.manual.sqlite
      fi
      rm -rf "$run"
      mkdir -p "$run"
      if $big; then
        mv /tmp/ressac-vm/.manual.sqlite "$run/r.sqlite"
      else
        cp "$repo/tests/fixtures/contract.sqlite" "$run/r.sqlite"
      fi
      if [ -n "$look" ]; then
        rm -rf "$keep"
        mkdir -p "$keep"
        if [ -f "$conf" ]; then
          cp "$conf" "$keep/saved.conf"
          echo "settings file saved (restored by manual stop)"
        else
          touch "$keep/absent"
          echo "settings file absent (removed by manual stop)"
        fi
        write_conf "" "$look"
      fi
      RESSAC_DB="$run/r.sqlite" QT_FORCE_STDERR_LOGGING=1 RESSAC_FRAME_LOG=$frame_log setsid nohup "$bin" \
        >"$run/run.log" 2>&1 </dev/null &
      sleep 3
      if pgrep -u "$VM_USER" -x ressac-kde >/dev/null; then
        echo "manual run started (log: $run/run.log)"
      else
        echo "ressac-kde did not stay up" >&2
        tail -n 20 "$run/run.log" >&2
        exit 1
      fi
      ;;
    *) bad_usage ;;
  esac
}

if [ "${1:-}" = --in-vm ]; then
  shift
  VM_USER=willow
  in_vm "$@"
  exit 0
fi

# ---------------------------------------------------------------------------------------------
# Mac side.
# ---------------------------------------------------------------------------------------------
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
pathfix='export PATH=/usr/local/bin:/opt/homebrew/bin:$PATH'

# vm_script root|user: runs the script read on stdin in the VM, as root or as the VM user.
vm_script() {
  # prlctl joins its arguments into one guest shell line without quoting, so no line carries a
  # quote: the script goes over standard input into a file (readable by the VM user), then runs.
  local who=$1 runner
  if [ "$who" = root ]; then
    runner='bash'
  else
    runner="runuser -u $VM_USER -- env HOME=$vm_home USER=$VM_USER LOGNAME=$VM_USER bash"
  fi
  ssh "$HOST" "$pathfix; prlctl exec '$VM' 'mkdir -p /tmp/ressac-vm; chmod 1777 /tmp/ressac-vm; cat > /tmp/ressac-vm/.script.sh; chmod 644 /tmp/ressac-vm/.script.sh'"
  ssh "$HOST" "$pathfix; prlctl exec '$VM' '$runner /tmp/ressac-vm/.script.sh'" </dev/null
}

vm_home_read() {
  vm_home=$(ssh "$HOST" "$pathfix; prlctl exec '$VM' getent passwd $VM_USER" | tr -d '\r' | cut -d: -f6)
  [ -n "$vm_home" ] || die "cannot read the VM user's home"
}

# send_tar ARCHIVE DIR: unpacks the gzipped tar ARCHIVE (on this Mac) into DIR on the VM (an
# existing directory of the VM user), by standard input when prlctl relays it, else by base64
# chunks.
send_tar() {
  local archive=$1 dir=$2 probe part
  probe=$(printf ok | ssh "$HOST" "$pathfix; prlctl exec '$VM' cat" 2>/dev/null || true)
  if [ "$probe" = ok ]; then
    echo "transfer path: standard input"
    ssh "$HOST" "$pathfix; prlctl exec '$VM' runuser -u $VM_USER -- tar -xzf - -C $dir" <"$archive"
  else
    echo "transfer path: base64 chunks"
    ssh "$HOST" "$pathfix; prlctl exec '$VM' rm -f /tmp/ressac-vm/.chunks.b64"
    base64 <"$archive" | tr -d '\n' | split -b 49152 - "$archive.part."
    for part in "$archive".part.*; do
      ssh "$HOST" "$pathfix; prlctl exec '$VM' 'cat >> /tmp/ressac-vm/.chunks.b64'" <"$part"
      rm -f "$part"
    done
    vm_script user <<EOF
base64 -d /tmp/ressac-vm/.chunks.b64 | tar -xzf - -C $dir
EOF
    # The chunks file is root's (prlctl exec), in a sticky directory: root removes it.
    ssh "$HOST" "$pathfix; prlctl exec '$VM' rm -f /tmp/ressac-vm/.chunks.b64"
  fi
}

# send_big LABEL N LONG: the fixture with N more xkcd entries (LONG=true: the first 200 carry a
# long article), built here and uploaded to /tmp/ressac-vm/LABEL/r.sqlite.
send_big() {
  local label=$1 n=$2 long=$3 tmp
  command -v sqlite3 >/dev/null || die "sqlite3 is needed on this Mac for --big"
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/ressac-big.XXXXXX")
  trap 'rm -rf "$tmp"' EXIT
  cp "$repo/tests/fixtures/contract.sqlite" "$tmp/big.sqlite"
  sqlite3 "$tmp/big.sqlite" "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < $n) INSERT INTO entry (feed_id, guid, url, title, published_at, fetched_at, summary_html) SELECT 1, 'big-' || i, 'https://example.org/big/' || i, 'Entry ' || i || ' of a long folder, with a title that wraps over two rows', strftime('%Y-%m-%dT%H:%M:%fZ', '2026-10-01', '-' || i || ' minutes'), '2026-10-01T00:00:00.000Z', '<p>Body ' || i || '</p>' FROM n;"
  if $long; then
    # The paragraph of tests/common/mod.rs `insert_entries`, 150 times (replace() over the 150
    # "00" pairs of a zeroed blob's hex).
    sqlite3 "$tmp/big.sqlite" <<'SQL'
UPDATE entry SET content_html = replace(hex(zeroblob(150)), '00',
  '<p>The tide comes in over the flats twice a day, and the <a href="https://example.org/">harbour</a> keeps its own <em>teletext</em> pages: every line is read, sanitised and laid out again.</p>')
WHERE feed_id = 1 AND guid LIKE 'big-%' AND CAST(substr(guid, 5) AS INTEGER) <= 200;
SQL
  fi
  mv "$tmp/big.sqlite" "$tmp/r.sqlite"
  echo "big database: $(sqlite3 "$tmp/r.sqlite" 'SELECT count(*) FROM entry WHERE feed_id = 1') entries in xkcd$($long && echo ', 200 long articles' || true)"
  COPYFILE_DISABLE=1 tar --no-xattrs -czf "$tmp/r.tar.gz" -C "$tmp" r.sqlite
  vm_script user <<EOF
rm -rf /tmp/ressac-vm/$label
mkdir -p /tmp/ressac-vm/$label
EOF
  send_tar "$tmp/r.tar.gz" "/tmp/ressac-vm/$label"
  rm -rf "$tmp"
  trap - EXIT
  echo "uploaded to /tmp/ressac-vm/$label/r.sqlite"
}

# check_options shots|manual ARGS...: validates the options; sets `big` (N or empty) and `long`.
check_options() {
  local mode=$1
  shift
  big=
  long=false
  while [ $# -gt 0 ]; do
    case "$mode:$1" in
      shots:--software-rendering | manual:--frame-log) ;;
      shots:--font | shots:--walk)
        [ $# -ge 2 ] || bad_usage
        shift
        ;;
      shots:--env)
        [ $# -ge 2 ] || bad_usage
        [[ $2 =~ ^[A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*$ ]] || bad_usage
        shift
        ;;
      *:--look)
        case "${2:-}" in antiope | native) ;; *) bad_usage ;; esac
        shift
        ;;
      *:--big)
        [[ ${2:-} =~ ^[1-9][0-9]{0,5}$ ]] || bad_usage
        big=$2
        shift
        ;;
      *:--long) long=true ;;
      *) bad_usage ;;
    esac
    shift
  done
  if $long && [ -z "$big" ]; then
    bad_usage
  fi
}

# in_vm_run MODE ARGS...: this very script runs on the VM (it may not be in the pushed commit yet).
in_vm_run() {
  {
    printf 'set -- --in-vm'
    printf ' %q' "$@"
    echo
    cat "${BASH_SOURCE[0]}"
  } | vm_script user
}

cmd=${1:-}
[ -n "$cmd" ] || bad_usage
shift

case "$cmd" in
  check)
    status=$(ssh "$HOST" "$pathfix; prlctl status '$VM'")
    echo "$status"
    case "$status" in
      *running*) ;;
      *)
        ssh "$HOST" "$pathfix; prlctl start '$VM'"
        up=false
        for _ in $(seq 1 36); do
          if ssh "$HOST" "$pathfix; prlctl exec '$VM' true" >/dev/null 2>&1; then
            up=true
            break
          fi
          sleep 5
        done
        $up || die "the VM did not answer within 180 s"
        ;;
    esac
    vm_home_read
    vm_script root <<EOF
head -n 1 /etc/fedora-release
uname -m
id $VM_USER
if [ -f $vm_home/.cache/ressac-build/installed-sha ]; then echo "installed: \$(cat $vm_home/.cache/ressac-build/installed-sha)"; else echo 'installed: none recorded'; fi
if pgrep -u $VM_USER -x plasmashell >/dev/null; then echo 'plasma session: yes'; else echo 'plasma session: no'; fi
EOF
    ;;

  push)
    [ $# -eq 0 ] || bad_usage
    vm_home_read
    if [ -n "$(git -C "$repo" status --porcelain)" ]; then
      echo "marghe.sh: warning, the work tree is dirty; only HEAD is sent" >&2
    fi
    sha=$(git -C "$repo" rev-parse --short HEAD)
    archive=$(mktemp "${TMPDIR:-/tmp}/ressac-push.XXXXXX")
    trap 'rm -f "$archive"' EXIT
    git -C "$repo" archive --format=tar.gz --prefix=ressac/ HEAD >"$archive"
    vm_script user <<EOF
rm -rf $vm_home/src/ressac.new
mkdir -p $vm_home/src/ressac.new
EOF
    send_tar "$archive" "$vm_home/src/ressac.new"
    vm_script user <<EOF
set -e
rm -rf $vm_home/src/ressac
mv $vm_home/src/ressac.new/ressac $vm_home/src/ressac
rmdir $vm_home/src/ressac.new
echo $sha > $vm_home/src/ressac/.pushed-sha
EOF
    mac_sum=$(git -C "$repo" show HEAD:Cargo.lock | shasum -a 256 | cut -d' ' -f1)
    vm_sum=$(echo "sha256sum $vm_home/src/ressac/Cargo.lock" | vm_script user | tr -d '\r' | cut -d' ' -f1)
    [ "$mac_sum" = "$vm_sum" ] || die "Cargo.lock differs on the VM ($vm_sum, expected $mac_sum)"
    echo "pushed $sha (Cargo.lock matches)"
    ;;

  install)
    case "${1:-}:$#" in
      :0 | --software-rendering:1) ;;
      *) bad_usage ;;
    esac
    vm_home_read
    start=$(date +%s)
    set +e
    # A release build (install.sh runs `cargo build --release`); the pushed commit is recorded
    # once installed, for `check`, `shots` and `manual`.
    vm_script user <<EOF
set -e
cd $vm_home/src/ressac
CARGO_TARGET_DIR=$vm_home/.cache/ressac-build apps/linux/install.sh ${1:-}
cp .pushed-sha $vm_home/.cache/ressac-build/installed-sha 2>/dev/null || rm -f $vm_home/.cache/ressac-build/installed-sha
EOF
    code=$?
    set -e
    echo "wall time: $(($(date +%s) - start)) s (exit $code)"
    exit "$code"
    ;;

  dnf)
    [ "${RESSAC_VM_DNF_OK:-}" = 1 ] || die "refusing to run dnf in the VM without RESSAC_VM_DNF_OK=1 (Clément's go-ahead)"
    [ $# -gt 0 ] || bad_usage
    ssh "$HOST" "$pathfix; prlctl exec '$VM' dnf install -y $*"
    ;;

  shots)
    label=${1:-}
    [[ $label =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || bad_usage
    [ "$label" != manual ] || bad_usage
    shift
    check_options shots "$@"
    vm_home_read
    if [ -n "$big" ]; then
      send_big "$label" "$big" "$long"
    fi
    set +e
    in_vm_run shots "$label" "$@"
    code=$?
    set -e
    dest=${TMPDIR:-/tmp}/ressac-vm/$label
    rm -rf "$dest"
    mkdir -p "$dest"
    if echo "tar -czf - -C /tmp/ressac-vm/$label . | base64 -w0" | vm_script user | tr -d '\r\n' | base64 -d | tar -xzf - -C "$dest"; then
      echo "pulled to $dest"
      grep -E '^(qml: )?(grid|frames): ' "$dest/run.log" || true
    else
      echo "marghe.sh: nothing pulled from /tmp/ressac-vm/$label" >&2
    fi
    exit "$code"
    ;;

  manual)
    if [ "${1:-}" = stop ]; then
      [ $# -eq 1 ] || bad_usage
      vm_home_read
      in_vm_run manual stop
      exit 0
    fi
    check_options manual "$@"
    vm_home_read
    if [ -n "$big" ]; then
      send_big manual "$big" "$long"
    fi
    in_vm_run manual "$@"
    ;;

  *) bad_usage ;;
esac
