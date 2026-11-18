#!/bin/sh
set -eu

repo="visible/sombre"

fail() {
  printf 'sombre: %s\n' "$1" >&2
  exit 1
}

main() {
  [ "$(uname -s)" = "Darwin" ] || fail "the desktop app is mac only for now. the cli works everywhere: curl -fsSL https://sombre.si/install | sh"
  if pgrep -x sombre-desktop >/dev/null 2>&1; then
    fail "quit the sombre app first, then run this again"
  fi

  base="https://github.com/$repo/releases/latest/download"
  archive="sombre.app.tar.gz"
  temp="$(mktemp -d)"
  trap 'rm -rf "$temp"' EXIT

  printf 'downloading the sombre app\n'
  curl -fsSL "$base/$archive" -o "$temp/$archive" || fail "couldn't download the app. check your connection and try again"
  curl -fsSL "$base/SHA256SUMS" -o "$temp/SHA256SUMS" || fail "couldn't download the checksums"

  expected="$(grep " $archive\$" "$temp/SHA256SUMS" | cut -d ' ' -f 1)"
  [ -n "$expected" ] || fail "the checksums don't list $archive"
  actual="$(shasum -a 256 "$temp/$archive" | cut -d ' ' -f 1)"
  [ "$expected" = "$actual" ] || fail "the download doesn't match its checksum, so nothing was installed"

  tar -xzf "$temp/$archive" -C "$temp"
  folder="/Applications"
  if [ ! -w "$folder" ]; then
    folder="$HOME/Applications"
    mkdir -p "$folder"
  fi
  rm -rf "$folder/sombre.app"
  mv "$temp/sombre.app" "$folder/sombre.app"

  printf 'installed the sombre app to %s. it updates itself from now on\n' "$folder/sombre.app"
  open "$folder/sombre.app"
}

main "$@"
