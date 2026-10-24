export type NavLink = {
  label: string;
  href: string;
  external?: boolean;
};

const repository = "https://github.com/t-aize/wyck";

export const siteConfig = {
  name: "wyck",
  tagline: "A terminal trading panel for cTrader, built for speed, not for staring at it.",
  repository,
  // Destinations that do not exist yet point to in-page anchors of sections still to build.
  nav: [
    { label: "Features", href: "#features" },
    { label: "Shortcuts", href: "#shortcuts" },
    { label: "Changelog", href: `${repository}/releases`, external: true },
    { label: "Docs", href: `${repository}#readme`, external: true },
  ] satisfies NavLink[],
  download: { label: "Download", href: `${repository}/releases/latest`, external: true },
  signIn: { label: "Sign in", href: "#sign-in" },
} as const;
