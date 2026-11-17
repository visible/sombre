```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│   / sombre                                                   │
│                                                              │
│   step away. the session comes with you.                     │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

```bash
> what is this?

  a desktop app for coding agents.
  start a session on your computer, keep it going from your phone.

> features?

  ✓ runs claude code and codex on macos, linux, and windows
  ✓ continue any session from your phone's browser
  ✓ approve commands and edits from anywhere
  ✓ one history per session, synced to both screens
  ✓ your code stays on your computer

> status?

  early, and usable every day. the desktop app, cli, and phone
  page all run claude code, codex, and sombre's own agent today.

> stack?

  apps/web        next.js · typescript · tailwind · gsap · bun             the site
  apps/desktop    tauri · react · typescript · tailwind                    the desktop app
  crates/core     rust                                                     session index and readers
  crates/harness  rust                                                     sombre's own agent
  crates/engine   rust                                                     runs a chat with any agent
  crates/cli      rust                                                     the sombre cli

> develop?

  bun install
  bun run dev                  the site
  bun run desktop              the desktop app, with live reload
  bun run app                  build the desktop app and install it
  cargo run -p sombre -- ls    the cli
  bun run fix                  format and fix lint
  bun run verify               lint, types, tests, and every build

> links?

  https://sombre.ai
  https://sombre.si

> license?

  mit, see LICENSE
```
