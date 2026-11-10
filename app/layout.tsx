import type { Metadata, Viewport } from "next";
import { Host_Grotesk, IBM_Plex_Mono } from "next/font/google";

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

const RootLayout = ({ children }: LayoutProps<"/">) => (
  <html className={`${host.variable} ${plex.variable} antialiased`} lang="en">
    <body className="min-h-dvh font-sans" suppressHydrationWarning>
      {children}
    </body>
  </html>
);

export default RootLayout;
