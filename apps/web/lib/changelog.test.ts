import { describe, expect, test } from "bun:test";

import { format, notes, releases } from "@/lib/changelog";

const parts = (version: string) => version.split(".").map(Number);

const newer = (one: string, two: string) => {
  const [left, right] = [parts(one), parts(two)];
  const index = left.findIndex((part, place) => part !== right[place]);
  return index !== -1 && (left[index] ?? 0) > (right[index] ?? 0);
};

describe("releases", () => {
  test("uses valid iso dates", () => {
    for (const release of releases) {
      expect(release.date).toMatch(/^\d{4}-\d{2}-\d{2}$/u);
      expect(Number.isNaN(Date.parse(release.date))).toBe(false);
    }
  });

  test("names each release by its version", () => {
    for (const release of releases) {
      expect(release.version).toMatch(/^\d+\.\d+\.\d+$/u);
    }
    expect(new Set(releases.map((release) => release.version)).size).toBe(
      releases.length
    );
  });

  test("lists the newest release first", () => {
    const dates = releases.map((release) => release.date);
    expect(dates).toEqual(dates.toSorted().toReversed());
    for (const [index, release] of releases.slice(1).entries()) {
      expect(newer(releases[index]?.version ?? "", release.version)).toBe(true);
    }
  });

  test("has a title and at least one new thing", () => {
    for (const release of releases) {
      expect(release.title.trim().length).toBeGreaterThan(0);
      expect(release.added.length).toBeGreaterThan(0);
    }
  });

  test("keeps every item to one plain line", () => {
    for (const item of releases.flatMap((release) => [
      ...release.added,
      ...release.fixed,
    ])) {
      expect(item).not.toMatch(/[\n`–—]/u);
    }
  });
});

describe("notes", () => {
  test("links a version to its release on github", () => {
    expect(notes("0.1.2")).toBe(
      "https://github.com/visible/sombre/releases/tag/v0.1.2"
    );
  });
});

describe("format", () => {
  test("shows a short readable date", () => {
    expect(format("2026-11-10")).toBe("Nov 10, 2026");
  });

  test("does not shift the day across time zones", () => {
    expect(format("2026-01-01")).toBe("Jan 1, 2026");
  });
});
