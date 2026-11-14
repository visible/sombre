import { describe, expect, test } from "bun:test";

import {
  advance,
  approve,
  end,
  finish,
  gate,
  initial,
  peek,
  start,
  view,
} from "@/lib/session";

describe("advance", () => {
  test("starts at the handoff", () => {
    expect(initial.live).toBe(start);
    expect(view(initial).title).toBe("Continued on phone");
  });

  test("moves forward and never back", () => {
    const edited = advance(initial, "edit");
    expect(edited.live).toBeGreaterThan(start);
    expect(advance(edited, "reply")).toBe(edited);
  });

  test("ignores events it does not know", () => {
    expect(advance(initial, "missing")).toBe(initial);
  });

  test("stops at the approval until the visitor approves", () => {
    expect(advance(initial, "suite").live).toBe(gate);
  });
});

describe("approval", () => {
  const waiting = advance(initial, "allow");

  test("asks before running the command", () => {
    expect(view(waiting)).toMatchObject({
      busy: false,
      chip: "approve",
      title: "Wants to run bun test",
    });
  });

  test("cannot be approved before the session asks", () => {
    expect(approve(initial)).toBe(initial);
  });

  test("runs, then reports the result", () => {
    const running = approve(waiting);
    expect(view(running)).toMatchObject({ busy: true, chip: "approved" });
    expect(advance(running, "suite").live).toBe(gate);
    const done = finish(running);
    expect(done.live).toBe(end);
    expect(view(done)).toMatchObject({
      busy: false,
      chip: "approved",
      meta: "48 passed",
      title: "All tests passed",
    });
  });

  test("finishing does nothing unless a command is running", () => {
    expect(finish(waiting)).toBe(waiting);
  });
});

describe("peek", () => {
  test("previews an earlier event without moving the session", () => {
    const peeking = peek(advance(initial, "allow"), 0);
    expect(peeking.live).toBe(gate);
    expect(view(peeking)).toMatchObject({
      chip: null,
      device: "mac",
      replay: true,
      time: "9:41",
    });
  });

  test("returns to the live event", () => {
    const back = peek(peek(initial, 2), null);
    expect(view(back).replay).toBe(false);
    expect(view(back).title).toBe("Continued on phone");
  });
});
