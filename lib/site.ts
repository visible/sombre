export const site = {
  description:
    "Sombre runs Claude Code and Codex on your Mac. Open the same session on your phone and keep working while you're away from your desk.",
  email: "hello@sombre.ai",
  name: "sombre",
  title: "sombre: step away, the session comes with you",
  url: "https://sombre.ai",
} as const;

export const access = `mailto:${site.email}?subject=${encodeURIComponent("Sombre early access")}`;
