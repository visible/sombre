export const site = {
  description:
    "Sombre runs Claude Code and Codex on your computer. Open the same session on your phone and keep working while you're away from your desk.",
  email: "hello@sombre.ai",
  name: "sombre",
  repo: "https://github.com/visible/sombre",
  title:
    "sombre: run claude code and codex on your mac, continue from your phone",
  url: "https://sombre.ai",
} as const;

export const access = `mailto:${site.email}?subject=${encodeURIComponent("Sombre early access")}`;

export const pages = [
  { href: "/docs", label: "Docs" },
  { href: "/security", label: "Security" },
  { href: "/changelog", label: "Changelog" },
] as const;
