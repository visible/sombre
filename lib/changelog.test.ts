import { describe, expect, test } from "bun:test";

import { format, releases } from "@/lib/changelog";

describe("releases", () => {
  test("uses valid iso dates", () => {
    for (const release of releases) {
      expect(release.date).toMatch(/^\d{4}-\d{2}-\d{2}$/u);
      expect(Number.isNaN(Date.parse(release.date))).toBe(false);
    }
  });

  test("lists the newest release first", () => {
    const dates = releases.map((release) => release.date);
    expect(dates).toEqual(dates.toSorted().toReversed());
  });

  test("has one release per date", () => {
    expect(new Set(releases.map((release) => release.date)).size).toBe(
      releases.length
    );
  });

  test("has a title and at least one item", () => {
    for (const release of releases) {
      expect(release.title.trim().length).toBeGreaterThan(0);
      expect(release.items.length).toBeGreaterThan(0);
    }
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
