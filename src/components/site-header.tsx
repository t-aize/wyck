import { ButtonLink } from "~/components/button-link";
import { Logo } from "~/components/logo";
import { MobileNav } from "~/components/mobile-nav";
import { siteConfig } from "~/config/site";

export function SiteHeader() {
  return (
    <header className="site-header fixed inset-x-0 top-0 z-40 h-19 px-[max(1rem,env(safe-area-inset-left))]">
      <div className="mx-auto flex h-full max-w-300 items-center gap-7">
        <Logo />
        <nav aria-label="Main" className="ml-auto hidden md:block">
          <ul className="flex gap-7 text-[0.9rem]">
            {siteConfig.nav.map((link) => (
              <li key={link.label}>
                <a
                  href={link.href}
                  {...("external" in link && link.external
                    ? { target: "_blank", rel: "noopener noreferrer" }
                    : {})}
                  className="link-underline text-paper-muted transition-colors duration-300 hover:text-white"
                >
                  {link.label}
                </a>
              </li>
            ))}
          </ul>
        </nav>
        <div className="ml-auto flex items-center gap-2 md:ml-0">
          <a
            href={siteConfig.signIn.href}
            className="hidden px-3 py-2.5 text-[0.9rem] text-paper-muted transition-colors duration-300 hover:text-white md:block"
          >
            {siteConfig.signIn.label}
          </a>
          <ButtonLink
            href={siteConfig.download.href}
            external={siteConfig.download.external}
            variant="secondary"
            size="md"
          >
            {siteConfig.download.label}
          </ButtonLink>
          <MobileNav />
        </div>
      </div>
    </header>
  );
}
