#!/bin/sh
set -eu

tag="$1"
previous="$(git describe --tags --abbrev=0 "$tag^" 2>/dev/null || true)"

list() {
  git log --reverse --format=%s "$previous..$tag" -- crates apps/desktop Cargo.toml Cargo.lock | sed -n "s/^$1: /- /p"
}

cat <<'EOF'
```sh
curl -fsSL https://sombre.si/install | sh
```

```powershell
irm https://sombre.si/install.ps1 | iex
```

the desktop app for mac:

```sh
curl -fsSL https://sombre.si/desktop | sh
```

or open `sombre.dmg` below. the cli and the app update themselves from here on.

EOF

if [ -z "$previous" ]; then
  cat <<'EOF'
## new

- one cli for claude code, codex, and sombre's own agent. `sombre resume` picks up any chat from any of them
- hand a chat to another agent partway through, with its history carried over
- `/pair` a phone through sombre.si, sealed end to end so the relay can't read a word. follow along, answer approvals, and send messages from the phone
- three permission levels, ask, auto, and yolo, switched with shift tab
- `/goal` keeps an agent working toward one outcome, `/compact` shrinks long chats, and `/model` sets the model and reasoning effort. cost and context left show after every turn
- `/device` keeps the computer awake, and running with the lid closed
- `!command` runs a shell command in the chat's folder and hands the output to the agent
- builds for macos, linux, and windows

EOF
else
  added="$(list feat)"
  fixed="$(list fix)"
  if [ -n "$added" ]; then
    printf '## new\n\n%s\n\n' "$added"
  fi
  if [ -n "$fixed" ]; then
    printf '## fixed\n\n%s\n\n' "$fixed"
  fi
fi

cat <<'EOF'
every file is in `SHA256SUMS`, which the installers check, and attested by this repo's release workflow: `gh attestation verify <file> -R visible/sombre`. updates only install when they're signed by sombre's release key
EOF
