import type { MetadataRoute } from "next";

import { pages, site } from "@/lib/site";

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { changeFrequency: "monthly", priority: 1, url: site.url },
    ...pages.map((page) => ({
      changeFrequency: "monthly" as const,
      priority: 0.8,
      url: `${site.url}${page.href}`,
    })),
  ];
}
