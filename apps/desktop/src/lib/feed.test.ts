import { describe, expect, test } from "bun:test";

import type { Chat, Entry } from "./api";
import { fresh, group, matches, recent, rounds, summary, target } from "./feed";

const step = (id: number, label: string): Entry => ({
  detail: "",
  failed: false,
  id,
  kind: "step",
  label,
  output: "",
  patch: [],
  step: String(id),
});

const chat: Chat = {
  agent: "codex",
  ago: "2h ago",
  folder: "/Users/sam/code/shop",
  id: "abc",
  place: "~/code/shop",
  project: "shop",
  title: "Fix the checkout total",
  updated: 0,
};

describe("group", () => {
  test("chains steps that follow each other", () => {
    const groups = group([
      { id: 0, kind: "user", text: "hi" },
      step(1, "Run"),
      step(2, "Read"),
      { id: 3, kind: "text", text: "done" },
      step(4, "Edit"),
    ]);
    expect(
      groups.map((part) =>
        part.kind === "work" ? part.steps.length : part.kind
      )
    ).toEqual(["single", 2, "single", 1]);
    expect(groups.map((part) => part.key)).toEqual([0, 1, 3, 4]);
  });
});

describe("matches", () => {
  test("finds every word anywhere, ignoring case", () => {
    expect(matches(chat, "")).toBe(true);
    expect(matches(chat, "checkout shop")).toBe(true);
    expect(matches(chat, "CODEX")).toBe(true);
    expect(matches(chat, "checkout claude")).toBe(false);
  });
});

describe("targets", () => {
  test("open a listed chat or a fresh one", () => {
    expect(target(chat, "codex:abc")).toEqual({
      agent: "codex",
      folder: {
        path: "/Users/sam/code/shop",
        place: "~/code/shop",
        project: "shop",
      },
      id: "abc",
      key: "codex:abc",
      place: "~/code/shop",
      title: "Fix the checkout total",
      updated: 0,
    });
    const folder = {
      path: "/Users/sam/code/shop",
      place: "~/code/shop",
      project: "shop",
    };
    expect(fresh("claude", folder, 7)).toEqual({
      agent: "claude",
      folder,
      id: null,
      key: "fresh:7",
      place: "~/code/shop",
      title: "new chat",
      updated: 0,
    });
  });
});

describe("turn changes", () => {
  const entries: Entry[] = [
    { id: 1, kind: "user", text: "fix it" },
    { files: [], from: "a", id: 2, kind: "changes", to: "b" },
    { cost: null, id: 3, kind: "done", millis: null },
    { files: [], from: "b", id: 4, kind: "changes", to: "c" },
  ];

  test("finds the latest turn's changes and counts finished turns", () => {
    expect(recent(entries)).toEqual({ from: "b", kind: "turn", to: "c" });
    expect(recent(entries.slice(0, 1))).toBeNull();
    expect(rounds(entries)).toBe(3);
  });
});

describe("summary", () => {
  test("says what a run of steps did in a sentence", () => {
    expect(
      summary([
        { label: "Run" },
        { label: "Read" },
        { label: "Run" },
        { label: "Edit" },
        { label: "MCP" },
      ])
    ).toBe("ran 2 commands, read 1 file, edited 1 file, 1 other step");
    expect(summary([{ label: "Find" }])).toBe("searched 1 time");
  });
});
