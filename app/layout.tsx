import type { Metadata, Viewport } from "next";
import { Host_Grotesk, IBM_Plex_Mono } from "next/font/google";

import { Footer } from "@/components/footer";
import { Header } from "@/components/header";
import { Motion } from "@/components/motion";
import { Pocket } from "@/components/pocket";
import { site } from "@/lib/site";

import "./globals.css";

const host = Host_Grotesk({
  subsets: ["latin"],
  variable: "--font-host",
});

const plex = IBM_Plex_Mono({
  subsets: ["latin"],
  variable: "--font-plex",
  weight: ["400", "500"],
});

export const metadata: Metadata = {
  alternates: { canonical: "./" },
  authors: [{ name: site.name }],
  creator: site.name,
  description: site.description,
  keywords: [
    "coding agent",
    "claude code",
    "codex",
    "remote",
    "mobile",
    "desktop app",
  ],
  metadataBase: new URL(site.url),
  openGraph: {
    description: site.description,
    images: [{ alt: site.name, height: 630, url: "/og.png", width: 1200 }],
    locale: "en_US",
    siteName: site.name,
    title: site.title,
    type: "website",
    url: site.url,
  },
  robots: {
    follow: true,
    googleBot: { follow: true, index: true },
    index: true,
  },
  title: { default: site.title, template: `%s | ${site.name}` },
  twitter: {
    card: "summary_large_image",
    description: site.description,
    images: ["/og.png"],
    title: site.title,
  },
};

export const viewport: Viewport = {
  colorScheme: "light",
  themeColor: "#edefef",
};

const data = JSON.stringify({
  "@context": "https://schema.org",
  "@type": "WebSite",
  description: site.description,
  name: site.name,
  publisher: {
    "@type": "Organization",
    email: site.email,
    name: site.name,
    url: site.url,
  },
  url: site.url,
}).replaceAll("<", "\\u003c");

const RootLayout = ({ children }: LayoutProps<"/">) => (
  <html
    className={`${host.variable} ${plex.variable} antialiased`}
    lang="en"
    suppressHydrationWarning
  >
    <body className="min-h-dvh font-sans" suppressHydrationWarning>
      <a className="skip" href="#main">
        Skip to content
      </a>
      <div className="border-line mx-auto max-w-[1280px] overflow-x-clip xl:border-x 2xl:max-w-[1440px]">
        <Header />
        <div id="stage">{children}</div>
        <Pocket />
        <Footer />
      </div>
      <script
        // oxlint-disable-next-line react/no-danger -- JSON-LD has to be inline
        dangerouslySetInnerHTML={{ __html: data }}
        type="application/ld+json"
      />
      <Motion />
    </body>
  </html>
);

export default RootLayout;
