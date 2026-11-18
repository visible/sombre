#!/bin/sh
set -eu

repo="visible/sombre"
base="https://github.com/$repo/releases/latest/download"

fail() {
  printf 'sombre: %s\n' "$1" >&2
  exit 1
}

fetch() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$2" "$1"
  else
    fail "install curl or wget, then run this again"
  fi
}

checked() {
  printf 'downloading the sombre app\n'
  fetch "$base/$1" "$temp/$1" || fail "couldn't download the app. check your connection and try again"
  fetch "$base/SHA256SUMS" "$temp/SHA256SUMS" || fail "couldn't download the checksums"
  expected="$(grep " $1\$" "$temp/SHA256SUMS" | cut -d ' ' -f 1)"
  [ -n "$expected" ] || fail "the checksums don't list $1"
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$temp/$1" | cut -d ' ' -f 1)"
  else
    actual="$(shasum -a 256 "$temp/$1" | cut -d ' ' -f 1)"
  fi
  [ "$expected" = "$actual" ] || fail "the download doesn't match its checksum, so nothing was installed"
}

mac() {
  checked sombre.app.tar.gz
  tar -xzf "$temp/sombre.app.tar.gz" -C "$temp"
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

linux() {
  case "$(uname -m)" in
    x86_64 | amd64) ;;
    *) fail "the desktop app is built for x86_64 linux for now. the cli runs on arm too: curl -fsSL https://sombre.si/install | sh" ;;
  esac
  if command -v apt-get >/dev/null 2>&1 && command -v dpkg >/dev/null 2>&1; then
    checked sombre.deb
    sudo apt-get install -y "$temp/sombre.deb"
    printf 'installed the sombre app. open it from your apps, or run sombre-desktop. it updates itself from now on\n'
  elif command -v dnf >/dev/null 2>&1; then
    checked sombre.rpm
    sudo dnf install -y "$temp/sombre.rpm"
    printf 'installed the sombre app. open it from your apps, or run sombre-desktop. it updates itself from now on\n'
  elif command -v zypper >/dev/null 2>&1; then
    checked sombre.rpm
    sudo zypper --non-interactive install --allow-unsigned-rpm "$temp/sombre.rpm"
    printf 'installed the sombre app. open it from your apps, or run sombre-desktop. it updates itself from now on\n'
  else
    checked sombre.AppImage
    folder="$HOME/Applications"
    mkdir -p "$folder"
    mv -f "$temp/sombre.AppImage" "$folder/sombre.AppImage"
    chmod +x "$folder/sombre.AppImage"
    printf 'installed the sombre app to %s. run it from there. it updates itself from now on\n' "$folder/sombre.AppImage"
  fi
}

main() {
  if pgrep -x sombre-desktop >/dev/null 2>&1; then
    fail "quit the sombre app first, then run this again"
  fi
  temp="$(mktemp -d)"
  trap 'rm -rf "$temp"' EXIT
  case "$(uname -s)" in
    Darwin) mac ;;
    Linux) linux ;;
    MINGW* | MSYS* | CYGWIN*) fail "on windows, run this in powershell instead: irm https://sombre.si/desktop.ps1 | iex" ;;
    *) fail "there's no sombre app for $(uname -s) yet" ;;
  esac
}

main "$@"
