import { siteConfig } from "~/config/site";

/** The wordmark is the logo. */
export function Logo() {
  return (
    <a
      href="/"
      aria-label={`${siteConfig.name}, home`}
      className="font-serif text-2xl leading-none tracking-[0.01em] italic"
    >
      {siteConfig.name}
    </a>
  );
}
