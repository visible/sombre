import type { Metadata } from "next";

import { Shell } from "@/components/shell";
import { site } from "@/lib/site";

export const metadata: Metadata = {
  description:
    "How Sombre keeps your code on your Mac while you continue sessions from your phone.",
  title: "Security",
};

const principles = [
  {
    body: "The agent runs on your machine, with your files and your tools. Sombre is designed so your repository never leaves your Mac.",
    title: "Your code stays on your Mac",
  },
  {
    body: "Your phone and your Mac talk through a relay that passes messages, approvals, and tool output between them. It doesn't run code or keep your files.",
    title: "The relay only carries messages",
  },
  {
    body: "Commands and file edits wait for your approval, on your Mac or from your phone. You decide what the agent is allowed to do.",
    title: "Nothing runs without your say",
  },
  {
    body: "A phone joins a session only when you scan the code on screen or open your sombre.si link. You can end the session from either device.",
    title: "Pairing is always explicit",
  },
] as const;

const Security = () => (
  <Shell
    facts={[
      { label: "Agent runs on", value: "Your Mac" },
      { label: "Code sent to relay", value: "None" },
      { label: "Runs without approval", value: "Nothing" },
    ]}
    lead="Sombre lets you leave your desk without handing your code to anyone. Here's how it's designed to keep that promise."
    tag="Security"
    title="Your code, your machine"
  >
    <ul className="border-line grid border-t md:grid-cols-2">
      {principles.map((item, index) => (
        <li
          className={`border-line border-b px-5 py-10 sm:px-8 lg:py-14 ${index % 2 === 0 ? "md:border-r" : ""} ${index > 1 ? "md:border-b-0" : ""} ${index === principles.length - 1 ? "border-b-0" : ""}`}
          key={item.title}
        >
          <h2 className="text-ink text-[clamp(1.35rem,2.2vw,1.75rem)] leading-tight font-medium tracking-[-0.025em]">
            {item.title}
          </h2>
          <p className="text-body mt-3 max-w-[44ch] text-[15px] leading-relaxed">
            {item.body}
          </p>
        </li>
      ))}
    </ul>
    <p className="border-line text-body border-t px-5 py-8 text-[15px] leading-relaxed sm:px-8">
      Sombre is in early development, and this page will add more detail as the
      first build ships. Questions about security?{" "}
      <a className="text-ink inline-block" href={`mailto:${site.email}`}>
        <span className="draw at-rest inline-block py-0.5">{site.email}</span>
      </a>
    </p>
  </Shell>
);

export default Security;
