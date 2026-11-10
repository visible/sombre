export type Kind = "user" | "agent" | "step" | "handoff" | "approval";

export type Icon = "search" | "run" | "edit";

export interface Entry {
  id: string;
  kind: Kind;
  text: string;
  pause: number;
  detail?: string;
  icon?: Icon;
  files?: string[];
}

export interface Scheduled extends Entry {
  delay: number;
  span: number;
}

export const thread: Entry[] = [
  {
    id: "ask",
    kind: "user",
    pause: 400,
    text: "The checkout test fails about one run in five. Can you find out why?",
  },
  {
    files: [
      "checkout.spec.ts",
      "payment.ts",
      "frame.tsx",
      "retry.ts",
      "setup.ts",
    ],
    icon: "search",
    id: "search",
    kind: "step",
    pause: 900,
    text: "Searched the codebase",
  },
  {
    detail: "4 of 20 failed",
    icon: "run",
    id: "repeat",
    kind: "step",
    pause: 1100,
    text: "Ran bun test checkout --repeat 20",
  },
  {
    id: "cause",
    kind: "agent",
    pause: 1300,
    text: "It waits a fixed 500ms for the payment frame. On slow runs the frame isn't ready yet, so I'd wait for its ready event instead.",
  },
  { id: "handoff", kind: "handoff", pause: 1800, text: "Continued on phone" },
  {
    id: "reply",
    kind: "user",
    pause: 1200,
    text: "Do it, then run the whole suite.",
  },
  {
    detail: "+2 -1",
    icon: "edit",
    id: "edit",
    kind: "step",
    pause: 900,
    text: "Edited tests/checkout.spec.ts",
  },
  {
    detail: "Approved from phone",
    id: "allow",
    kind: "approval",
    pause: 1000,
    text: "Run bun test",
  },
  {
    detail: "48 passed",
    icon: "run",
    id: "suite",
    kind: "step",
    pause: 1000,
    text: "Ran bun test",
  },
  {
    id: "done",
    kind: "agent",
    pause: 1400,
    text: "Fixed. 20 repeat runs of the checkout test passed with no failures.",
  },
];

export const schedule = (entries: Entry[]): Scheduled[] => {
  let elapsed = 0;
  const delays = entries.map((entry) => {
    elapsed += entry.pause;
    return elapsed;
  });
  return entries.map((entry, index) => {
    const delay = delays[index] ?? 0;
    const next = delays[index + 1];
    return { ...entry, delay, span: next === undefined ? 0 : next - delay };
  });
};

export const moment = (entries: Scheduled[], kind: Kind): number =>
  entries.find((entry) => entry.kind === kind)?.delay ?? 0;

export const linked = (entries: Scheduled[], index: number): boolean =>
  entries[index]?.kind === "step" && entries[index + 1]?.kind === "step";
