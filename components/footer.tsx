import Link from "next/link";

import { Access } from "@/components/access";
import { Horizon } from "@/components/horizon";
import { Tag } from "@/components/tag";
import { Top } from "@/components/top";
import { access, pages, site } from "@/lib/site";

const columns = [
  { links: pages, title: "Product" },
  {
    links: [
      { href: "/#how", label: "How it works" },
      { href: "/#history", label: "Session history" },
      { href: "/#why", label: "Why Sombre" },
      { href: "/#questions", label: "Questions" },
    ],
    title: "Explore",
  },
  {
    links: [
      { href: access, label: "Early access" },
      { href: `mailto:${site.email}`, label: "Email us" },
    ],
    title: "Say hello",
  },
] as const;

const style =
  "text-ink decoration-ink/30 hover:decoration-ink text-[15px] underline-offset-4 transition-colors hover:underline";

export const Footer = () => (
  <footer className="border-line border-t">
    <div className="grid lg:grid-cols-12">
      <div className="border-line px-5 py-16 sm:px-8 lg:col-span-7 lg:border-r lg:py-24">
        <Tag>Early access</Tag>
        <h2 className="text-ink mt-6 max-w-[15ch] text-[clamp(2.4rem,4.8vw,4.2rem)] leading-[1.02] font-medium tracking-[-0.035em] text-balance">
          Take your next session for a walk.
        </h2>
        <p className="text-body mt-5 max-w-[38ch] text-[15px] leading-relaxed">
          Sombre is in early development. We&apos;ll email you when the first
          build is ready.
        </p>
        <div className="mt-8">
          <Access />
        </div>
      </div>
      <nav
        aria-label="Footer"
        className="border-line grid grid-cols-2 content-start gap-8 border-t px-5 py-12 sm:grid-cols-3 sm:px-8 lg:col-span-5 lg:border-t-0 lg:py-24 lg:pl-12"
      >
        {columns.map((column) => (
          <div key={column.title}>
            <h3 className="text-meta font-mono text-[11px] tracking-[0.06em] uppercase">
              {column.title}
            </h3>
            <ul className="mt-4 space-y-2.5">
              {column.links.map((link) => (
                <li key={link.label}>
                  {link.href.startsWith("/") ? (
                    <Link className={style} href={link.href}>
                      {link.label}
                    </Link>
                  ) : (
                    <a className={style} href={link.href}>
                      {link.label}
                    </a>
                  )}
                </li>
              ))}
            </ul>
          </div>
        ))}
      </nav>
    </div>
    <div className="border-line relative h-[240px] overflow-hidden border-t sm:h-[340px] lg:h-[420px]">
      <Horizon />
    </div>
    <div className="border-line grid items-center gap-4 border-t px-5 py-6 sm:px-8 md:grid-cols-[1fr_auto_1fr]">
      <p className="text-meta order-3 text-[13px] md:order-none">
        &copy; 2026 Sombre
      </p>
      <a
        className="group text-ink order-1 justify-self-start text-[clamp(1.4rem,3.2vw,2.1rem)] font-medium tracking-[-0.02em] md:order-none md:justify-self-center"
        href={`mailto:${site.email}`}
      >
        <span className="bg-[linear-gradient(var(--color-ember),var(--color-ember))] bg-[length:0%_2px] bg-[position:0_100%] bg-no-repeat pb-1 transition-[background-size] duration-500 ease-out group-hover:bg-[length:100%_2px] motion-reduce:transition-none">
          {site.email}
        </span>
      </a>
      <Top />
    </div>
  </footer>
);
