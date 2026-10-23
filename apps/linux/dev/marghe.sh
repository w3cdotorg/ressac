#!/usr/bin/env bash
# Drives Clément's Fedora KDE VM (Parallels "Fedora Linux" on his Mac marghe) from this Mac:
# transfer the committed tree, build and install it there, run the screenshot walk inside his live
# Plasma session, hand-check with `manual`. Used for the VM checkpoints of sub-project 8.
#   apps/linux/dev/marghe.sh check
#   apps/linux/dev/marghe.sh push
#   apps/linux/dev/marghe.sh install [--software-rendering]
#   RESSAC_VM_DNF_OK=1 apps/linux/dev/marghe.sh dnf <package...>
#   apps/linux/dev/marghe.sh shots <label> [--software-rendering] [--font <name>]
#   apps/linux/dev/marghe.sh manual [stop]
# Everything on the VM stays under ~/src/ressac, ~/.cache/ressac-build, ~/.local and /tmp/ressac-vm.
# shellcheck disable=SC2029,SC2016
set -euo pipefail

HOST=willow@marghe.local
VM="Fedora Linux"
VM_USER=willow
usage='usage: marghe.sh check | push | install [--software-rendering] | dnf <package...> | shots <label> [--software-rendering] [--font systemSans|systemSerif|newsreader|atkinson] | manual [stop]'

die() {
  echo "marghe.sh: $*" >&2
  exit 1
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
    XDG_CURRENT_DESKTOP QT_QPA_PLATFORMTHEME KDE_FULL_SESSION DISPLAY; do
    value=$(tr '\0' '\n' <"/proc/$pid/environ" | sed -n "s/^$name=//p" | head -1 || true)
    if [ -n "$value" ]; then
      export "$name=$value"
    fi
  done
}

in_vm() {
  mode=$1
  shift
  repo=$HOME/src/ressac
  bin="$HOME/.local/bin/ressac-kde"
  case "$mode" in
    shots)
      label=$1
      shift
      software=false
      font=
      while [ $# -gt 0 ]; do
        case "$1" in
          --software-rendering) software=true ;;
          --font)
            font=$2
            shift
            ;;
          *) echo "$usage" >&2; exit 2 ;;
        esac
        shift
      done
      session_env
      run=/tmp/ressac-vm/$label
      rm -rf "$run"
      mkdir -p "$run/shots"
      cp "$repo/tests/fixtures/contract.sqlite" "$run/r.sqlite"
      conf=$HOME/.config/ressac/ressac-kde.conf
      saved=$run/saved.conf
      had_conf=false
      if [ -f "$conf" ]; then
        had_conf=true
        cp "$conf" "$saved"
        echo "settings file saved"
      else
        echo "settings file absent"
      fi
      if [ -n "$font" ]; then
        mkdir -p "$(dirname "$conf")"
        printf '[General]\nreader.font=%s\n' "$font" >"$conf"
      fi
      extra=()
      if $software; then
        extra=(QT_QUICK_BACKEND=software QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu)
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
      ;;
    manual)
      session_env
      run=/tmp/ressac-vm/manual
      if [ "${1:-}" = stop ]; then
        pkill -u "$VM_USER" -x ressac-kde || true
        rm -rf "$run"
        echo "manual run stopped"
        exit 0
      fi
      rm -rf "$run"
      mkdir -p "$run"
      cp "$repo/tests/fixtures/contract.sqlite" "$run/r.sqlite"
      RESSAC_DB="$run/r.sqlite" QT_FORCE_STDERR_LOGGING=1 setsid nohup "$bin" \
        >"$run/run.log" 2>&1 </dev/null &
      sleep 3
      if pgrep -u "$VM_USER" -x ressac-kde >/dev/null; then
        echo "manual run started"
      else
        echo "ressac-kde did not stay up" >&2
        tail -n 20 "$run/run.log" >&2
        exit 1
      fi
      ;;
    *) echo "$usage" >&2; exit 2 ;;
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

cmd=${1:-}
[ -n "$cmd" ] || {
  echo "$usage" >&2
  exit 2
}
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
if pgrep -u $VM_USER -x plasmashell >/dev/null; then echo 'plasma session: yes'; else echo 'plasma session: no'; fi
EOF
    ;;

  push)
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
    probe=$(printf ok | ssh "$HOST" "$pathfix; prlctl exec '$VM' cat" 2>/dev/null || true)
    if [ "$probe" = ok ]; then
      echo "transfer path: standard input"
      ssh "$HOST" "$pathfix; prlctl exec '$VM' runuser -u $VM_USER -- tar -xzf - -C $vm_home/src/ressac.new" <"$archive"
    else
      echo "transfer path: base64 chunks"
      ssh "$HOST" "$pathfix; prlctl exec '$VM' rm -f /tmp/ressac.tar.gz.b64"
      base64 <"$archive" | tr -d '\n' | split -b 49152 - "$archive.part."
      for part in "$archive".part.*; do
        ssh "$HOST" "$pathfix; prlctl exec '$VM' 'cat >> /tmp/ressac.tar.gz.b64'" <"$part"
        rm -f "$part"
      done
      vm_script user <<EOF
base64 -d /tmp/ressac.tar.gz.b64 | tar -xzf - -C $vm_home/src/ressac.new
rm -f /tmp/ressac.tar.gz.b64
EOF
    fi
    vm_script user <<EOF
set -e
rm -rf $vm_home/src/ressac
mv $vm_home/src/ressac.new/ressac $vm_home/src/ressac
rmdir $vm_home/src/ressac.new
EOF
    mac_sum=$(git -C "$repo" show HEAD:Cargo.lock | shasum -a 256 | cut -d' ' -f1)
    vm_sum=$(echo "sha256sum $vm_home/src/ressac/Cargo.lock" | vm_script user | tr -d '\r' | cut -d' ' -f1)
    [ "$mac_sum" = "$vm_sum" ] || die "Cargo.lock differs on the VM ($vm_sum, expected $mac_sum)"
    echo "pushed $sha (Cargo.lock matches)"
    ;;

  install)
    case "${1:-}:$#" in
      :0 | --software-rendering:1) ;;
      *)
        echo "$usage" >&2
        exit 2
        ;;
    esac
    vm_home_read
    start=$(date +%s)
    set +e
    vm_script user <<EOF
cd $vm_home/src/ressac && CARGO_TARGET_DIR=$vm_home/.cache/ressac-build apps/linux/install.sh ${1:-}
EOF
    code=$?
    set -e
    echo "wall time: $(($(date +%s) - start)) s (exit $code)"
    exit "$code"
    ;;

  dnf)
    [ "${RESSAC_VM_DNF_OK:-}" = 1 ] || die "refusing to run dnf in the VM without RESSAC_VM_DNF_OK=1 (Clément's go-ahead)"
    [ $# -gt 0 ] || {
      echo "$usage" >&2
      exit 2
    }
    ssh "$HOST" "$pathfix; prlctl exec '$VM' dnf install -y $*"
    ;;

  shots)
    label=${1:-}
    [ -n "$label" ] || {
      echo "$usage" >&2
      exit 2
    }
    shift
    vm_home_read
    set +e
    # This very script runs on the VM (it may not be in the pushed commit yet).
    {
      echo "set -- --in-vm shots $label $*"
      cat "${BASH_SOURCE[0]}"
    } | vm_script user
    code=$?
    set -e
    dest=${TMPDIR:-/tmp}/ressac-vm/$label
    rm -rf "$dest"
    mkdir -p "$dest"
    echo "tar -czf - -C /tmp/ressac-vm/$label . | base64 -w0" | vm_script user | tr -d '\r\n' | base64 -d | tar -xzf - -C "$dest"
    echo "pulled to $dest"
    exit "$code"
    ;;

  manual)
    vm_home_read
    {
      echo "set -- --in-vm manual ${1:-}"
      cat "${BASH_SOURCE[0]}"
    } | vm_script user
    ;;

  *)
    echo "$usage" >&2
    exit 2
    ;;
esac
