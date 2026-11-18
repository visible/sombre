import { site } from "@/lib/site";

export interface Release {
  version: string;
  date: string;
  title: string;
  added: string[];
  fixed: string[];
}

export const releases: Release[] = [
  {
    added: [
      "The desktop app reads easier, with IBM Plex Sans for prose, your messages as cards, and a roomier sidebar grouped by project",
      "Chats open instantly in the desktop app",
      "Settings open as pages, and About checks for updates",
      "A cleaner header and composer in the desktop app, and dialogs you can close with an x",
      "/update installs the newest Sombre from inside a chat",
      "Every turn ends with how long it took instead of what it cost, on your computer and your phone",
      "Errors lead with a plain headline and dim the details beneath it",
    ],
    date: "2026-11-18",
    fixed: [
      "Sombre starts right away when another chat is already open, instead of waiting five seconds",
      "Your phone shows a chat's earlier messages after /resume or /new, and keeps up when you switch agent or model",
      "/resume lists only the chats of the agent you're using",
      "Approval and question cards always fit the screen in the CLI",
      "Plain errors when Claude Code, Codex, or a provider can't be reached",
      "Dozens of small CLI fixes for menus, resizing, long pastes, and narrow terminals",
    ],
    title: "A calmer, faster desktop app",
    version: "0.1.2",
  },
  {
    added: [
      "The desktop app for Mac, Windows, and Linux, with a pairing code for each chat",
      "The CLI and the desktop app update themselves from signed releases",
      "Agent questions show in the CLI, the desktop app, and on your phone, and are never answered for you",
      "When the CLI quits, your phone carries on in the desktop app on the same link",
      "A new look for the CLI, with a one line header, a bar beside every message, and a footer you arrange",
      "Provider keys stay in the Windows Credential Manager, and Linux runs agent commands in a sandbox",
    ],
    date: "2026-11-18",
    fixed: [
      "Tighter phone pairing: links expire, forgotten phones are cut off, and replayed messages are refused",
      "The agent stays inside what each permission mode allows",
      "The phone page fits small screens, with no sideways scrolling",
      "A handoff between agents that fails partway can be retried without losing the chat",
    ],
    title: "The desktop app, and updates that install themselves",
    version: "0.1.1",
  },
  {
    added: [
      "One CLI for Claude Code, Codex, and Sombre's own agent, and sombre resume picks up any chat from any of them",
      "Hand a chat to another agent partway through, with its history",
      "Pair your phone through sombre.si, sealed end to end so the relay can't read a word",
      "Three permission levels, ask, auto, and yolo, switched with shift tab",
      "/goal keeps an agent working toward one outcome, and /device keeps your computer awake with the lid closed",
      "Builds for macOS, Linux, and Windows, each checked and attested by the release workflow",
      "sombre.ai, with a live demo of a session moving from Mac to phone, plus docs, security, and this changelog",
    ],
    date: "2026-11-17",
    fixed: [],
    title: "The first release",
    version: "0.1.0",
  },
];

export const notes = (version: string) =>
  `${site.repo}/releases/tag/v${version}`;

export const format = (date: string) =>
  new Date(`${date}T12:00:00Z`).toLocaleDateString("en-US", {
    day: "numeric",
    month: "short",
    timeZone: "UTC",
    year: "numeric",
  });
