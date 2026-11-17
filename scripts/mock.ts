import { readFileSync } from "node:fs";

import { base, open, seal } from "../crates/cli/src/phone.js";

interface Image {
  name: string;
  data: string;
}

interface Message {
  kind: string;
  text?: string;
  now?: boolean;
  id?: string;
  yes?: boolean;
  answers?: string[] | null;
  images?: Image[];
}

const source = new URL("../crates/cli/src/", import.meta.url);
const key = crypto.getRandomValues(new Uint8Array(32));
const out: string[] = [];
const seen = new Set<string>();
const waiters: (() => void)[] = [];
let offline = false;

const push = (value: unknown) => {
  out.push(seal(key, value));
  for (const wake of waiters.splice(0)) {
    wake();
  }
};

const steps: [string, string][] = [
  ["Run", "git status --short"],
  ["Read", "src/main.rs"],
  ["Read", "src/lib.rs"],
  ["Find", "fn total"],
  ["Run", "cargo build"],
  ["Edit", "src/lib.rs"],
  ["Run", "cargo test"],
];

const answer = (message: Message) => {
  const images = (message.images ?? []).map((image) => ({
    name: image.name,
    src: `data:image/jpeg;base64,${image.data}`,
  }));
  const marks = images.map((_, index) => `[Image #${index + 1}]`).join(" ");
  push({
    images,
    kind: "user",
    text: [message.text, marks].filter(Boolean).join("\n"),
  });
  if (message.text?.includes("ask")) {
    push({
      id: "q1",
      kind: "questions",
      questions: [
        {
          choices: [
            { about: "The brand orange", label: "Ember" },
            { about: "Plain and dark", label: "Ink" },
          ],
          header: "Color",
          multiple: false,
          text: "Which color should the button be?",
        },
        {
          choices: [
            { about: "", label: "Home" },
            { about: "", label: "Docs" },
          ],
          header: "Pages",
          multiple: true,
          text: "Which pages need it?",
        },
      ],
    });
    if (message.text.includes("mac")) {
      setTimeout(
        () => push({ answers: ["Ink", "Home"], id: "q1", kind: "answered" }),
        2500
      );
    }
    return;
  }
  if (message.text?.includes("approve")) {
    push({
      change: ["-rm -rf build", "+rm -rf target/debug"],
      detail: "rm -rf target/debug",
      id: "a1",
      kind: "approval",
      tool: "Bash",
    });
    if (message.text.includes("mac")) {
      setTimeout(() => push({ id: "a1", kind: "decided", yes: true }), 2500);
    }
    return;
  }
  const words =
    "Fixed it. `total` was renamed to `sum` in `lib.rs`, so one call site broke. The build and tests pass now.".split(
      " "
    );
  const timeline: unknown[] = [
    ...steps.map(([label, detail]) => ({ detail, kind: "step", label })),
    {
      kind: "output",
      text: "error[E0425]: cannot find value `total` in this scope\n --> src/lib.rs:42:9",
    },
    { detail: "cargo test", kind: "step", label: "Run" },
    ...words.map((word) => ({ kind: "delta", text: `${word} ` })),
    { kind: "text", text: words.join(" ") },
    { cost: 0.03, kind: "done", left: 72 },
  ];
  for (const [index, item] of timeline.entries()) {
    setTimeout(() => push(item), 200 + index * 120);
  }
};

push({
  agent: "claude code",
  folder: "demo",
  kind: "hello",
  model: "opus",
  name: "mock chat",
});

const server = Bun.serve({
  async fetch(request) {
    const url = new URL(request.url);
    const path = url.pathname;
    if (path === "/app.js") {
      return new Response(readFileSync(new URL("phone.js", source)), {
        headers: { "content-type": "text/javascript" },
      });
    }
    if (path === "/offline") {
      offline = !offline;
      return new Response(
        offline ? "offline: every request now fails" : "online again"
      );
    }
    if (path === "/r/mock") {
      return new Response(readFileSync(new URL("phone.html", source)), {
        headers: { "content-type": "text/html" },
      });
    }
    if (offline) {
      await Bun.sleep(400);
      return new Response("offline", { status: 502 });
    }
    if (path === "/r/mock/pair") {
      return new Response("mock-phone");
    }
    if (path === "/r/mock/events") {
      const after = Number(url.searchParams.get("after") ?? 0);
      if (after >= out.length) {
        const { promise, resolve } = Promise.withResolvers<null>();
        waiters.push(() => resolve(null));
        await Promise.race([promise, Bun.sleep(20_000)]);
      }
      return Response.json({
        epoch: "mock",
        items: out.slice(after),
        next: out.length,
      });
    }
    if (path === "/r/mock/send") {
      const id = url.searchParams.get("id") ?? "";
      const message: Message | null = open(key, await request.text());
      if (!message || seen.has(id)) {
        return new Response("ok");
      }
      seen.add(id);
      if (message.kind === "message") {
        answer(message);
      } else if (message.kind === "reply") {
        push({
          answers: message.answers ?? null,
          id: message.id,
          kind: "answered",
        });
        push({
          kind: "text",
          text: message.answers
            ? `You picked ${message.answers.join(" and ")}.`
            : "Skipped. Tell me what to do instead.",
        });
        push({ cost: 0.01, kind: "done", left: 72 });
      } else if (message.kind === "answer") {
        push({ id: message.id, kind: "decided", yes: message.yes });
        push({ cost: 0.01, kind: "done", left: 72 });
      }
      return new Response("ok");
    }
    return new Response("not found", { status: 404 });
  },
  hostname: "0.0.0.0",
  idleTimeout: 30,
  port: Number(process.env.PORT ?? 4321),
});

console.log(
  `phone page: http://localhost:${server.port}/r/mock#k=${base(key)}&t=mock`
);
console.log(
  `drop the connection: http://localhost:${server.port}/offline (open it again to come back)`
);
console.log(
  'send "approve" to see an approval card, "approve mac" to see it answered on the mac'
);
console.log(
  'send "ask" to see a question card, "ask mac" to see it answered on the mac'
);
