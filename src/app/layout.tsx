import type { Metadata, Viewport } from "next";
import { SiteHeader } from "~/components/site-header";
import { siteConfig } from "~/config/site";
import { fontVariables } from "./fonts";
import "./globals.css";

export const metadata: Metadata = {
  title: siteConfig.name,
  description: siteConfig.tagline,
};

export const viewport: Viewport = {
  colorScheme: "dark",
  themeColor: "#11120e",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className={`${fontVariables} h-full antialiased`}>
      <body className="flex min-h-full flex-col">
        <a
          href="#content"
          className="fixed top-3 left-3 z-50 -translate-y-24 rounded-xs bg-paper px-4 py-2 font-semibold text-ink focus-visible:translate-y-0"
        >
          Skip to content
        </a>
        <SiteHeader />
        <main id="content" className="flex flex-1 flex-col">
          {children}
        </main>
      </body>
    </html>
  );
}
