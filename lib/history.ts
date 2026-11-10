import type { Row } from "@/lib/terminal";

export type Device = "mac" | "phone";

export type Screen =
  | { kind: "status"; text: string }
  | { kind: "message"; text: string }
  | { kind: "approval"; command: string };

export interface Event {
  id: string;
  time: string;
  device: Device;
  title: string;
  detail: string;
  meta?: string;
  rows: Row[];
  phone?: Screen;
}

export const devices: Record<Device, string> = {
  mac: "On your Mac",
  phone: "From your phone",
};

export const history: Event[] = [
  {
    detail:
      "The checkout test fails about one run in five. Can you find out why?",
    device: "mac",
    id: "ask",
    rows: [
      { text: "claude", tone: "command" },
      {
        text: "The checkout test fails about one run in five. Why?",
        tone: "ask",
      },
    ],
    time: "9:41",
    title: "Asked about the flaky checkout test",
  },
  {
    detail: "Read checkout.spec.ts, payment.ts, frame.tsx, and 2 more files.",
    device: "mac",
    id: "search",
    meta: "5 files",
    rows: [
      { text: "Searching the codebase", tone: "command" },
      { aside: "214 lines", text: "tests/checkout.spec.ts", tone: "output" },
      { aside: "88 lines", text: "src/payment.ts", tone: "output" },
      { aside: "1.2s", text: "Read 5 files", tone: "done" },
    ],
    time: "9:42",
    title: "Searched the codebase",
  },
  {
    detail: "Ran the checkout test 20 times in a row to reproduce the failure.",
    device: "mac",
    id: "repeat",
    meta: "4 of 20 failed",
    rows: [
      { text: "bun test checkout --repeat 20", tone: "command" },
      { aside: "fail", text: "4 of 20 runs", tone: "remove" },
      { text: "Reproduced the failure", tone: "done" },
    ],
    time: "9:43",
    title: "Reproduced the failure",
  },
  {
    detail:
      "The test waits a fixed 500ms for the payment frame instead of its ready event.",
    device: "mac",
    id: "cause",
    rows: [
      {
        aside: "line 42",
        text: "await page.waitForTimeout(500)",
        tone: "remove",
      },
      { text: "Waits a fixed 500ms for the payment frame", tone: "done" },
      { text: "Wait for its ready event instead?", tone: "ask" },
    ],
    time: "9:44",
    title: "Found the cause",
  },
  {
    detail: "Opened the session from sombre.si on the walk to lunch.",
    device: "phone",
    id: "handoff",
    phone: { kind: "status", text: "sombre.si/k4x2" },
    rows: [{ aside: "sombre.si/k4x2", text: "Phone connected", tone: "done" }],
    time: "12:08",
    title: "Continued on phone",
  },
  {
    detail: "Do it, then run the whole suite.",
    device: "phone",
    id: "reply",
    phone: { kind: "message", text: "Do it, then run the whole suite." },
    rows: [
      {
        aside: "from phone",
        text: "Do it, then run the whole suite.",
        tone: "ask",
      },
    ],
    time: "12:08",
    title: "Replied to the agent",
  },
  {
    detail:
      "Replaced the fixed timeout with a wait for the frame's ready event.",
    device: "mac",
    id: "edit",
    meta: "+2 -1",
    rows: [
      { text: "Editing tests/checkout.spec.ts", tone: "command" },
      { text: "await page.waitForTimeout(500)", tone: "remove" },
      { text: "await frame.locator('[data-ready]').waitFor()", tone: "add" },
      { aside: "+2 -1", text: "Saved", tone: "done" },
    ],
    time: "12:09",
    title: "Edited tests/checkout.spec.ts",
  },
  {
    detail: "Allowed the agent to run bun test with one tap.",
    device: "phone",
    id: "allow",
    phone: { command: "bun test", kind: "approval" },
    rows: [{ aside: "from phone", text: "Approved bun test", tone: "done" }],
    time: "12:09",
    title: "Approved bun test",
  },
  {
    detail: "The full suite passed, and 20 repeat runs of checkout passed too.",
    device: "mac",
    id: "suite",
    meta: "48 passed",
    rows: [
      { text: "bun test", tone: "command" },
      { aside: "48 pass", text: "All tests passed", tone: "done" },
    ],
    time: "12:10",
    title: "All tests passed",
  },
];
