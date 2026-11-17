import { describe, expect, test } from "bun:test";

import {
  BURST,
  DAILY,
  DAY,
  KEPT,
  LARGEST,
  MINUTES,
  allowed,
  append,
  cursor,
  empty,
  frame,
  hex,
  label,
  machines,
  network,
  pack,
  rebuild,
  room,
  since,
  strings,
  total,
} from "./log";

describe("log", () => {
  test("hands back only what a reader hasn't seen", () => {
    const log = append(empty(), ["a", "b", "c"]);
    expect(since(log, 0)).toEqual({ items: ["a", "b", "c"], next: 3 });
    expect(since(log, 2)).toEqual({ items: ["c"], next: 3 });
    expect(since(log, 3)).toEqual({ items: [], next: 3 });
    expect(since(log, 9)).toEqual({ items: [], next: 3 });
  });

  test("keeps the newest items and remembers how many fell off", () => {
    const log = append(append(empty(), ["a", "b"], 3), ["c", "d"], 3);
    expect(log).toEqual({ base: 1, items: ["b", "c", "d"] });
    expect(total(log)).toBe(4);
    expect(since(log, 0)).toEqual({ items: ["b", "c", "d"], next: 4 });
  });

  test("batches items into few storage rows", () => {
    expect(pack(["aa", "bb", "cc"], 4)).toEqual([["aa", "bb"], ["cc"]]);
    expect(pack(["huge".repeat(10)], 4)).toEqual([["huge".repeat(10)]]);
    expect(pack([], 4)).toEqual([]);
    expect(pack(["x".repeat(LARGEST + 1), "ok"])).toEqual([["", "ok"]]);
  });

  test("rebuilds the window from stored rows in any order", () => {
    const rows = [
      { first: 2, items: ["c", "d"] },
      { first: 0, items: ["a", "b"] },
    ];
    expect(rebuild(rows, 0)).toEqual({ base: 0, items: ["a", "b", "c", "d"] });
    expect(rebuild(rows, 0, 3)).toEqual({ base: 1, items: ["b", "c", "d"] });
    expect(rebuild([], 7)).toEqual({ base: 7, items: [] });
    expect(KEPT).toBeGreaterThan(1000);
  });
});

describe("protocol", () => {
  test("reads room ids and actions from paths", () => {
    const id = "0123456789abcdef01234567";
    expect(room(`/r/${id}`)).toEqual({ action: "", id });
    expect(room(`/r/${id}/events`)).toEqual({ action: "events", id });
    expect(room("/r/short/events")).toBeNull();
    expect(room(`/r/${id}/../x`)).toBeNull();
  });

  test("reads what the mac sends over its socket", () => {
    expect(frame('{"out":"abc"}')).toEqual({
      fresh: false,
      out: ["abc"],
      read: null,
    });
    expect(frame('{"out":["a","b",3,""],"fresh":true}')).toEqual({
      fresh: true,
      out: ["a", "b"],
      read: null,
    });
    expect(frame('{"read":12}')).toEqual({ fresh: false, out: [], read: 12 });
    expect(frame('{"read":-1}').read).toBeNull();
    expect(frame("ping")).toEqual({ fresh: false, out: [], read: null });
  });

  test("reads cursors and falls back on anything else", () => {
    expect(cursor("12", 0)).toBe(12);
    expect(cursor(null, 5)).toBe(5);
    expect(cursor("", 5)).toBe(5);
    expect(cursor(7, 5)).toBe(7);
    expect(cursor("-3", 5)).toBe(5);
    expect(cursor("1.5", 5)).toBe(5);
    expect(cursor("abc", 5)).toBe(5);
  });

  test("makes random hex ids of the right length", () => {
    expect(hex(12)).toMatch(/^[0-9a-f]{24}$/u);
    expect(hex(12)).not.toBe(hex(12));
  });
});

describe("stored state", () => {
  test("reads saved lists and drops anything malformed", () => {
    expect(strings('["a",2,"b"]')).toEqual(["a", "b"]);
    expect(strings("nope")).toEqual([]);
    expect(strings(null)).toEqual([]);
    expect(
      machines('[{"id":"d1","label":"iPhone, Safari","since":5},{"id":2}]')
    ).toEqual([{ id: "d1", label: "iPhone, Safari", since: 5 }]);
  });

  test("names a phone the same way the mac's relay does", () => {
    expect(
      label("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0) AppleWebKit Safari/604.1")
    ).toBe("iPhone, Safari");
    expect(
      label("Mozilla/5.0 (Linux; Android 16) Chrome/140 Mobile Safari")
    ).toBe("Android, Chrome");
    expect(label("curl/8")).toBe("browser");
  });
});

describe("new pairings", () => {
  test("allows a burst, then waits for the window to pass", () => {
    const now = 1_000_000_000;
    const burst = Array.from(
      { length: BURST },
      (_, index) => now - index * 1000
    );
    expect(allowed(burst.slice(1), now)).toBe(true);
    expect(allowed(burst, now)).toBe(false);
    expect(allowed(burst, now + MINUTES)).toBe(true);
  });

  test("caps a day no matter how slowly they come", () => {
    const now = 5_000_000_000;
    const day = Array.from(
      { length: DAILY },
      (_, index) => now - (index + 1) * MINUTES
    );
    expect(allowed(day.slice(1), now)).toBe(true);
    expect(allowed(day, now)).toBe(false);
    expect(allowed(day, now + DAY)).toBe(true);
  });

  test("counts a whole ipv6 /64 as one network", () => {
    expect(network("2001:DB8:1:2:aaaa:bbbb:cccc:dddd")).toBe("2001:db8:1:2");
    expect(network("2001:db8:1:2:1111:2222:3333:4444")).toBe("2001:db8:1:2");
    expect(network("203.0.113.9")).toBe("203.0.113.9");
  });
});
