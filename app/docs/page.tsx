import type { Metadata } from "next";

import { Access } from "@/components/access";
import { Shell } from "@/components/shell";

export const metadata: Metadata = {
  description:
    "Guides for installing Sombre, pairing your phone, and running Claude Code and Codex sessions.",
  title: "Docs",
};

const topics = [
  {
    body: "Download the Mac app and sign in to the agents you already use.",
    title: "Install Sombre",
  },
  {
    body: "Scan the code on screen or open your sombre.si link.",
    title: "Pair your phone",
  },
  {
    body: "Pick a project and an agent, then start working.",
    title: "Start a session",
  },
  {
    body: "Allow or deny commands and file edits from either device.",
    title: "Approve changes",
  },
  {
    body: "Set up Claude Code or Codex and switch between them.",
    title: "Choose an agent",
  },
  {
    body: "Fix pairing, connection, and agent sign-in problems.",
    title: "Troubleshooting",
  },
] as const;

const Docs = () => (
  <Shell
    lead="Docs arrive with the first build. Here's what they'll cover, and you can get early access to hear when they're ready."
    tag="Docs"
    title="Guides for every step"
  >
    <ul className="bg-line border-line grid gap-px border-t sm:grid-cols-2 lg:grid-cols-3">
      {topics.map((topic) => (
        <li
          className="bg-paper flex flex-col gap-3 px-5 py-8 sm:px-8"
          key={topic.title}
        >
          <span className="text-meta border-line w-fit rounded-full border px-2 py-0.5 font-mono text-[10px]">
            soon
          </span>
          <h2 className="text-ink text-[19px] font-medium tracking-[-0.01em]">
            {topic.title}
          </h2>
          <p className="text-body text-[15px] leading-relaxed">{topic.body}</p>
        </li>
      ))}
    </ul>
    <div className="border-line border-t px-5 py-10 sm:px-8">
      <Access />
    </div>
  </Shell>
);

export default Docs;
