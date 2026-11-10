export interface Release {
  date: string;
  title: string;
  items: string[];
}

export const releases: Release[] = [
  {
    date: "2026-11-10",
    items: [
      "sombre.ai and sombre.si now point to the site",
      "Session history replays the whole session as you scrub through it",
      "New line logo and favicon",
      "Docs, Security, and Changelog pages",
    ],
    title: "sombre.ai is live",
  },
  {
    date: "2026-11-09",
    items: [
      "Hero with a live demo of a session moving from Mac to phone",
      "How it works, features, and questions",
      "Interactive sunset in the footer",
    ],
    title: "First landing page",
  },
];

export const format = (date: string) =>
  new Date(`${date}T12:00:00Z`).toLocaleDateString("en-US", {
    day: "numeric",
    month: "short",
    timeZone: "UTC",
    year: "numeric",
  });
