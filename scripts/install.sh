#!/bin/sh
set -eu

repo="NVFP4/chrome-debloat"
binary="chrome-debloat"
tmpdir=""
platform=""
arch=""
asset=""
download_url=""
archive=""
app=""

red='\033[1;31m'
yellow='\033[0;33m'
cyan='\033[0;36m'
reset='\033[0m'

err() {
  printf '%bERROR%b: %s\n' "$red" "$reset" "$1" >&2
  exit 1
}

info() {
  printf '%b%s%b\n' "$cyan" "$1" "$reset"
}

warn() {
  printf '%b%s%b\n' "$yellow" "$1" "$reset"
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    err "need '$1' (command not found)"
  fi
}

have() {
  command -v "$1" >/dev/null 2>&1
}

download() {
  if have curl; then
    curl -fsSL "$1" -o "$2"
  elif have wget; then
    wget --https-only -qO "$2" "$1"
  else
    err "need 'curl' or 'wget' (command not found)"
  fi
}

cleanup() {
  if [ -n "$tmpdir" ]; then
    rm -rf "$tmpdir"
  fi
}

interrupt() {
  cleanup
  exit 130
}

terminate() {
  cleanup
  exit 143
}

trap cleanup EXIT
trap interrupt INT
trap terminate TERM

detect_platform() {
  case "$(uname -s)" in
    Linux)
      platform="linux"
      ;;
    Darwin)
      platform="macos"
      ;;
    *)
      err "unsupported operating system: $(uname -s)"
      ;;
  esac
}

detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64)
      arch="x86_64"
      ;;
    arm64|aarch64)
      arch="aarch64"
      ;;
    *)
      err "unsupported CPU architecture: $(uname -m)"
      ;;
  esac
}

download_app() {
  tmpdir="$(mktemp -d)"
  asset="$binary-$platform-$arch.tar.gz"
  download_url="https://github.com/$repo/releases/latest/download/$asset"
  archive="$tmpdir/$asset"
  app="$tmpdir/$binary"

  info "Downloading $asset..."
  download "$download_url" "$archive"
  tar -xzf "$archive" -C "$tmpdir"
  chmod +x "$app"
}

run_app() {
  if [ "$platform" != "linux" ]; then
    exec "$app"
    return
  fi

  if [ "$(id -u)" = "0" ]; then
    exec "$app"
  else
    need sudo
    warn "Chrome Debloat needs sudo to write browser policies in /etc."
    exec sudo "$app"
  fi
}

main() {
  need tar
  detect_platform
  detect_arch
  download_app
  run_app
}

main "$@"