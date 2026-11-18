import { describe, expect, test } from "bun:test";

import type { Chat } from "./api";
import {
  WIDTH,
  blank,
  keyed,
  parse,
  places,
  sections,
  shelves,
  short,
  toggle,
  widen,
} from "./sidebar";

const chat = (id: string, folder: string, title = id): Chat => ({
  agent: "claude",
  ago: "2h ago",
  folder,
  id,
  place: folder.replace("/Users/sam", "~"),
  project: folder.split("/").at(-1) ?? folder,
  title,
  updated: 0,
});

const many = Array.from({ length: 8 }, (_, index) =>
  chat(`a${index}`, "/Users/sam/app")
);

describe("sections", () => {
  test("groups chats by project, newest project first", () => {
    const result = sections(
      [
        chat("b1", "/Users/sam/site"),
        chat("a1", "/Users/sam/app"),
        chat("b2", "/Users/sam/site"),
      ],
      blank,
      "",
      null
    );
    expect(
      result.projects.map((project) => [project.name, project.chats.length])
    ).toEqual([
      ["site", 2],
      ["app", 1],
    ]);
  });

  test("shows five per project and keeps the open chat visible", () => {
    const [folded] = sections(many, blank, "", null).projects;
    expect(folded?.chats.length).toBe(5);
    expect(folded?.more).toBe(3);
    const [open] = sections(many, blank, "", "claude:a7").projects;
    expect(open?.chats.map((item) => item.id)).toContain("a7");
    expect(open?.more).toBe(2);
    const [opened] = sections(
      many,
      { ...blank, opened: ["/Users/sam/app"] },
      "",
      null
    ).projects;
    expect(opened?.more).toBe(0);
  });

  test("lifts pinned chats out and hides archived ones", () => {
    const result = sections(
      [
        chat("a1", "/Users/sam/app"),
        chat("a2", "/Users/sam/app"),
        chat("a3", "/Users/sam/app"),
      ],
      { ...blank, archived: ["claude:a3"], pinned: ["claude:a2"] },
      "",
      null
    );
    expect(result.pinned.map((item) => item.id)).toEqual(["a2"]);
    expect(result.projects[0]?.chats.map((item) => item.id)).toEqual(["a1"]);
    expect(result.total).toBe(2);
  });

  test("searching shows every match", () => {
    const result = sections(many, blank, "a", null);
    expect(result.projects[0]?.chats.length).toBe(8);
  });
});

const now = 2_000_000_000;

const at = (id: string, folder: string, updated: number) => ({
  ...chat(id, folder),
  updated,
});

describe("inbox", () => {
  test("sorts chats onto shelves and keeps pins on top", () => {
    const list = [
      at("a1", "/Users/sam/app", now - 60),
      at("a2", "/Users/sam/app", now - 60),
      at("a3", "/Users/sam/app", now - 30 * 86_400),
      at("b1", "/Users/sam/site", now - 120),
    ];
    const prefs = {
      ...blank,
      pinned: ["claude:a2"],
      snoozed: { "claude:b1": now + 600 },
    };
    const found = shelves(list, prefs, "", () => null, now);
    expect(found.pinned.map((item) => item.id)).toEqual(["a2"]);
    expect(found.inbox.map((item) => item.id)).toEqual(["a1"]);
    expect(found.snoozed.map((item) => item.id)).toEqual(["b1"]);
    expect(found.settled.map((item) => item.id)).toEqual(["a3"]);
    expect(found.total).toBe(4);
  });

  test("the project filter narrows every view", () => {
    const list = [
      at("a1", "/Users/sam/app", now),
      at("b1", "/Users/sam/site", now),
    ];
    const only = { ...blank, only: ["/Users/sam/site"] };
    expect(
      shelves(list, only, "", () => null, now).inbox.map((item) => item.id)
    ).toEqual(["b1"]);
    expect(
      sections(list, only, "", null).projects.map((item) => item.folder)
    ).toEqual(["/Users/sam/site"]);
  });

  test("lists each project once with its chat count", () => {
    const list = [
      at("a1", "/Users/sam/app", now),
      at("a2", "/Users/sam/app", now),
      at("b1", "/Users/sam/site", now),
    ];
    expect(places(list, { ...blank, archived: ["claude:a2"] })).toEqual([
      { count: 1, folder: "/Users/sam/app", name: "app", place: "~/app" },
      { count: 1, folder: "/Users/sam/site", name: "site", place: "~/site" },
    ]);
  });
});

describe("prefs", () => {
  test("reads saved prefs and survives junk", () => {
    expect(parse(null)).toEqual(blank);
    expect(parse("not json")).toEqual(blank);
    expect(parse('{"pinned":["claude:a",3],"hidden":true}')).toEqual({
      ...blank,
      hidden: true,
      pinned: ["claude:a"],
    });
  });

  test("reads the inbox marks and drops anything that isn't a time", () => {
    const saved = parse(
      JSON.stringify({
        flat: true,
        only: ["/w/app", 4],
        settled: { "claude:a": 100, "claude:b": "soon" },
        shelves: ["settled"],
        snoozed: { "codex:c": 200 },
      })
    );
    expect(saved.settled).toEqual({ "claude:a": 100 });
    expect(saved.snoozed).toEqual({ "codex:c": 200 });
    expect(saved.only).toEqual(["/w/app"]);
    expect(saved.shelves).toEqual(["settled"]);
    expect(saved.flat).toBe(true);
    expect(parse(JSON.stringify({ grouped: false })).flat).toBe(false);
  });

  test("toggles entries in a list", () => {
    expect(toggle(["a"], "b")).toEqual(["a", "b"]);
    expect(toggle(["a", "b"], "a")).toEqual(["b"]);
  });

  test("keys and short times", () => {
    expect(keyed(chat("x", "/a"))).toBe("claude:x");
    expect(short("2h ago")).toBe("2h");
    expect(short("now")).toBe("now");
  });
});

describe("width", () => {
  test("keeps the sidebar between its narrowest and widest", () => {
    expect(widen(100)).toBe(220);
    expect(widen(300.4)).toBe(300);
    expect(widen(900)).toBe(420);
    expect(parse(JSON.stringify({ width: 350 })).width).toBe(350);
    expect(parse(JSON.stringify({ width: "wide" })).width).toBe(WIDTH);
    expect(parse(null).width).toBe(WIDTH);
  });
});
