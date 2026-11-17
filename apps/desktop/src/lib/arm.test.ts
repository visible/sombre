import { describe, expect, test } from "bun:test";

import { ARM, armed, typing } from "./arm";

describe("typing", () => {
  test("spots text fields and editable elements", () => {
    expect(typing({ tagName: "TEXTAREA" })).toBe(true);
    expect(typing({ tagName: "INPUT" })).toBe(true);
    expect(typing({ tagName: "SELECT" })).toBe(true);
    expect(typing({ isContentEditable: true, tagName: "DIV" })).toBe(true);
  });

  test("ignores everything else", () => {
    expect(typing({ tagName: "BODY" })).toBe(false);
    expect(typing({ tagName: "BUTTON" })).toBe(false);
    expect(typing({ isContentEditable: false, tagName: "DIV" })).toBe(false);
    expect(typing(null)).toBe(false);
    expect(typing(undefined)).toBe(false);
  });
});

describe("armed", () => {
  test("waits a moment before taking an answer", () => {
    expect(armed(1000, 1000)).toBe(false);
    expect(armed(1000, 1000 + ARM - 1)).toBe(false);
    expect(armed(1000, 1000 + ARM)).toBe(true);
  });

  test("stays shut until the card has a start time", () => {
    expect(armed(Number.POSITIVE_INFINITY, 1_000_000)).toBe(false);
  });
});
