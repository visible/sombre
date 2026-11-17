#!/bin/sh
set -eu

tag="$1"
previous="$(git describe --tags --abbrev=0 "$tag^" 2>/dev/null || true)"

list() {
  git log --reverse --format=%s "$previous..$tag" -- crates Cargo.toml Cargo.lock | sed -n "s/^$1: /- /p"
}

cat <<'EOF'
```sh
curl -fsSL https://sombre.si/install | sh
```

```powershell
irm https://sombre.si/install.ps1 | iex
```

EOF

if [ -z "$previous" ]; then
  printf '## new\n\n- the first release: run claude code, codex, or sombre'"'"'s own agent in any folder, and /pair a phone to keep going when you step away\n\n'
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
every file is in `SHA256SUMS`, which the installers check, and attested by this repo's release workflow: `gh attestation verify <file> -R visible/sombre`
EOF
