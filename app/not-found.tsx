import type { Metadata } from "next";
import Link from "next/link";

import { Shell } from "@/components/shell";

export const metadata: Metadata = {
  description: "This page isn't part of the session. Head back to Sombre.",
  title: "Page not found",
};

const Missing = () => (
  <Shell
    facts={[
      { label: "Status", value: "404" },
      { label: "Session", value: "Still running" },
    ]}
    lead="Nothing lives at this address. The session in your pocket is still running, so pick it up where you left it."
    tag="404"
    title="This page stepped away"
  >
    <div className="border-line border-t px-5 py-10 sm:px-8" data-pocket="">
      <Link
        className="pill bg-ink text-sheet inline-flex h-11 items-center justify-center rounded-full px-5 text-[15px] font-medium"
        href="/"
      >
        <span className="roll">
          <span>Back to the start</span>
          <span aria-hidden="true">Back to the start</span>
        </span>
      </Link>
    </div>
  </Shell>
);

export default Missing;
