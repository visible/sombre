import { describe, expect, test } from "bun:test";

import type { Question } from "./api";
import { blank, elsewise, toggle, value } from "./quiz";

const question = (multiple: boolean): Question => ({
  choices: [
    { about: "the brand orange", label: "Ember" },
    { about: "plain and dark", label: "Ink" },
  ],
  header: "Color",
  multiple,
  text: "Which color should the button be?",
});

describe("quiz", () => {
  test("a single choice keeps only the latest pick", () => {
    const one = question(false);
    const draft = toggle(toggle(blank(), "Ember", false), "Ink", false);
    expect(value(one, draft)).toBe("Ink");
    expect(value(one, blank())).toBeNull();
  });

  test("several choices join in the order they were offered", () => {
    const many = question(true);
    const draft = toggle(toggle(blank(), "Ink", true), "Ember", true);
    expect(value(many, draft)).toBe("Ember, Ink");
    expect(value(many, toggle(draft, "Ink", true))).toBe("Ember");
  });

  test("something else needs typed words before it counts", () => {
    const one = question(false);
    const open = elsewise(toggle(blank(), "Ember", false), false);
    expect(open.picked).toEqual([]);
    expect(value(one, open)).toBeNull();
    expect(value(one, { ...open, typed: "  teal  " })).toBe("teal");
    const many = question(true);
    const mixed = {
      ...elsewise(toggle(blank(), "Ink", true), true),
      typed: "red",
    };
    expect(value(many, mixed)).toBe("Ink, red");
  });
});
