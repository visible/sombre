#!/bin/sh
set -eu

repo="visible/sombre"
folder="${SOMBRE_INSTALL:-$HOME/.local/bin}"

fail() {
  printf 'sombre: %s\n' "$1" >&2
  exit 1
}

main() {
  case "$(uname -s)" in
    Darwin) system="apple-darwin" ;;
    Linux) system="unknown-linux-musl" ;;
    MINGW* | MSYS* | CYGWIN*) fail "on windows, run this in powershell instead: irm https://sombre.si/install.ps1 | iex" ;;
    *) fail "there's no build for $(uname -s) yet" ;;
  esac

  case "$(uname -m)" in
    arm64 | aarch64) machine="aarch64" ;;
    x86_64 | amd64) machine="x86_64" ;;
    *) fail "there's no build for $(uname -m) yet" ;;
  esac

  if [ "$system" = "apple-darwin" ] && [ "$machine" = "x86_64" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
    machine="aarch64"
  fi

  target="$machine-$system"
  archive="sombre-$target.tar.gz"
  base="https://github.com/$repo/releases/latest/download"
  temp="$(mktemp -d)"
  trap 'rm -rf "$temp"' EXIT

  fetch() {
    if command -v curl >/dev/null 2>&1; then
      curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
      wget -qO "$2" "$1"
    else
      fail "install curl or wget, then run this again"
    fi
  }

  printf 'downloading sombre for %s\n' "$target"
  fetch "$base/$archive" "$temp/$archive" || fail "couldn't download $archive. check your connection and try again"
  fetch "$base/SHA256SUMS" "$temp/SHA256SUMS" || fail "couldn't download the checksums"

  expected="$(grep " $archive\$" "$temp/SHA256SUMS" | cut -d ' ' -f 1)"
  [ -n "$expected" ] || fail "the checksums don't list $archive"
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$temp/$archive" | cut -d ' ' -f 1)"
  else
    actual="$(shasum -a 256 "$temp/$archive" | cut -d ' ' -f 1)"
  fi
  [ "$expected" = "$actual" ] || fail "the download doesn't match its checksum, so nothing was installed"

  tar -xzf "$temp/$archive" -C "$temp"
  mkdir -p "$folder"
  mv -f "$temp/sombre" "$folder/sombre"
  chmod +x "$folder/sombre"

  printf 'installed %s to %s\n' "$("$folder/sombre" --version)" "$folder/sombre"
  case ":$PATH:" in
    *":$folder:"*) printf 'run sombre in a project folder to start\n' ;;
    *) printf 'add %s to your PATH, then run sombre in a project folder\n' "$folder" ;;
  esac
}

main "$@"
