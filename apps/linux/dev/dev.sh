#!/usr/bin/env bash
# The Fedora 44 aarch64 dev container on OrbStack. It only ever touches the
# container, image and volumes named below.
set -euo pipefail

export PATH="$HOME/.orbstack/bin:/opt/homebrew/bin:$PATH"

NAME=ressac-fedora
IMAGE=ressac-fedora:44
TARGET_VOLUME=ressac-fedora-target
CARGO_VOLUME=ressac-fedora-cargo
PLATFORM=linux/arm64
MOUNT=/work/ressac

dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$dir/../../.." && pwd -P)

usage() {
  echo "usage: apps/linux/dev/dev.sh image|up|sh [command…]|check" >&2
  exit 2
}

preflight() {
  local context
  context=$(docker context show 2>/dev/null || true)
  if [ "$context" != orbstack ]; then
    echo "dev.sh: the docker context is $context, not orbstack" >&2
    exit 1
  fi
  if ! docker info >/dev/null 2>&1; then
    echo "dev.sh: OrbStack is not running (open -a OrbStack)" >&2
    exit 1
  fi
}

image_id() { docker image inspect -f '{{.Id}}' "$IMAGE" 2>/dev/null || true; }

build_image() {
  docker build --pull --platform "$PLATFORM" \
    --build-arg USER_UID="$(id -u)" \
    -t "$IMAGE" -f "$dir/Containerfile" "$dir"
}

run_container() {
  docker run -d --init --platform "$PLATFORM" \
    --name "$NAME" --hostname "$NAME" \
    -v "$repo:$MOUNT" \
    -v "$TARGET_VOLUME:/home/ressac/target" \
    -v "$CARGO_VOLUME:/home/ressac/.cargo" \
    -w "$MOUNT" "$IMAGE" sleep infinity >/dev/null
}

up() {
  preflight
  [ -n "$(image_id)" ] || build_image
  local want have state
  want=$(image_id)
  have=$(docker inspect -f '{{.Image}}' "$NAME" 2>/dev/null || true)
  if [ -z "$have" ]; then
    run_container
  elif [ "$have" != "$want" ]; then
    docker rm -f "$NAME" >/dev/null
    run_container
  else
    state=$(docker inspect -f '{{.State.Running}}' "$NAME")
    [ "$state" = true ] || docker start "$NAME" >/dev/null
  fi
  want=$(image_id)
  want=${want#sha256:}
  echo "$NAME is running (image ${want:0:12})."
}

tty_flag() {
  if [ -t 0 ] && [ -t 1 ]; then echo -it; else echo -i; fi
}

sh_in_container() {
  up
  local tty
  tty=$(tty_flag)
  if [ "$#" -eq 0 ]; then
    docker exec "$tty" -u ressac -w "$MOUNT" "$NAME" bash -l
  else
    docker exec "$tty" -u ressac -w "$MOUNT" "$NAME" bash -lc "$*"
  fi
}

[ "$#" -ge 1 ] || usage
cmd=$1
shift
case "$cmd" in
  image) preflight; build_image ;;
  up) up ;;
  sh) sh_in_container "$@" ;;
  *) usage ;;
esac
