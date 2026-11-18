import { describe, expect, test } from "bun:test";

import { clip, diff, encode, pictures, short, took } from "./format";

describe("took", () => {
  test("reads like the cli's clock", () => {
    expect(took(null)).toBe("");
    expect(took(400)).toBe("<1s");
    expect(took(6200)).toBe("6s");
    expect(took(130_000)).toBe("2m 10s");
    expect(took(3_900_000)).toBe("1h 05m");
  });
});

describe("short", () => {
  test("drops the claude family prefix like the cli", () => {
    expect(short("claude-opus-5-5")).toBe("opus 5-5");
    expect(short("opus")).toBe("opus");
    expect(short("gpt-6.2-sol")).toBe("gpt-6.2-sol");
  });
});

describe("clip", () => {
  test("keeps the first lines and counts the rest", () => {
    expect(clip("a\nb\nc\n", 2)).toEqual({ hidden: 1, lines: ["a", "b"] });
    expect(clip("a", 5)).toEqual({ hidden: 0, lines: ["a"] });
  });
});

describe("diff", () => {
  test("keys and tones each line", () => {
    expect(diff(["+new", "-old", " same"])).toEqual([
      { key: 0, text: "+new", tone: "add" },
      { key: 1, text: "-old", tone: "gone" },
      { key: 2, text: " same", tone: "same" },
    ]);
  });
});

describe("images", () => {
  test("encodes large images in chunks", () => {
    const data = Uint8Array.from(
      { length: 100_000 },
      (_, index) => index % 256
    );
    expect(encode(data)).toBe(Buffer.from(data).toString("base64"));
  });

  test("keeps only image paths from a drop", () => {
    expect(
      pictures(["/a/shot.PNG", "/a/notes.md", "/a/photo.jpeg", "/a/clip.webp"])
    ).toEqual(["/a/shot.PNG", "/a/photo.jpeg", "/a/clip.webp"]);
  });
});
