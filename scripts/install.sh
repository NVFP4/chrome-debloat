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
checksum=""
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
  printf '%bINFO%b: %s\n' "$cyan" "$reset" "$1" >&2
}

warn() {
  printf '%bWARN%b: %s\n' "$yellow" "$reset" "$1" >&2
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
    tmpdir=""
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
  checksum="$archive.sha256"
  app="$tmpdir/$binary"

  info "Downloading '$asset'"
  download "$download_url" "$archive"
  download "$download_url.sha256" "$checksum"
  info "Verifying '$asset'"
  verify_checksum
  tar -xzf "$archive" -C "$tmpdir"
  chmod +x "$app"
}

verify_checksum() {
  expected="$(awk '{print $1; exit}' "$checksum" | tr '[:upper:]' '[:lower:]')"

  if [ -z "$expected" ]; then
    err "checksum file for $asset was empty"
  fi

  if have sha256sum; then
    actual="$(sha256sum "$archive" | awk '{print $1}' | tr '[:upper:]' '[:lower:]')"
  elif have shasum; then
    actual="$(shasum -a 256 "$archive" | awk '{print $1}' | tr '[:upper:]' '[:lower:]')"
  elif have openssl; then
    actual="$(openssl dgst -sha256 "$archive" | awk '{print $NF}' | tr '[:upper:]' '[:lower:]')"
  else
    err "need 'sha256sum', 'shasum', or 'openssl' to verify downloads"
  fi

  if [ "$actual" != "$expected" ]; then
    err "checksum mismatch for $asset"
  fi
}

run_app() {
  # dont exec here, we need to cleanup after app exit
  if [ "$platform" != "linux" ]; then
    "$app"
    return
  fi

  if [ "$(id -u)" = "0" ]; then
    "$app"
  else
    need sudo
    warn "Chrome Debloat needs sudo to write browser policies in /etc."
    sudo "$app"
  fi
}

main() {
  need tar
  detect_platform
  detect_arch
  download_app
  run_app
  info 'BYE!'
}

main "$@"
