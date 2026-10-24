"use client";

import { useEffect, useId, useRef, useState } from "react";
import { siteConfig } from "~/config/site";

// Disclosure pattern: a toggle button with aria-expanded controlling a panel of links.
export function MobileNav() {
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const close = (restoreFocus: boolean) => {
      setOpen(false);
      if (restoreFocus) {
        buttonRef.current?.focus();
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        close(true);
      }
    };
    // The panel only exists below the md breakpoint; close it when the viewport grows past it.
    const desktop = window.matchMedia("(min-width: 48rem)");
    const onDesktop = () => {
      if (desktop.matches) {
        close(false);
      }
    };
    // Back/forward and hash navigation are not link clicks; close on those too.
    const onNavigate = () => close(false);
    document.addEventListener("keydown", onKeyDown);
    desktop.addEventListener("change", onDesktop);
    window.addEventListener("hashchange", onNavigate);
    window.addEventListener("popstate", onNavigate);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      desktop.removeEventListener("change", onDesktop);
      window.removeEventListener("hashchange", onNavigate);
      window.removeEventListener("popstate", onNavigate);
    };
  }, [open]);

  return (
    <div className="md:hidden">
      <button
        ref={buttonRef}
        type="button"
        aria-expanded={open}
        aria-controls={panelId}
        aria-label="Menu"
        onClick={() => setOpen((value) => !value)}
        className="grid size-11 place-items-center rounded-xs text-paper-muted hover:text-white"
      >
        <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true">
          <path
            d={open ? "M4 4l12 12M16 4L4 16" : "M3 6h14M3 10h14M3 14h14"}
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
          />
        </svg>
      </button>
      <nav
        id={panelId}
        aria-label="Main"
        hidden={!open}
        className="absolute inset-x-0 top-full border-b border-paper/10 bg-ink/95 px-[max(1rem,env(safe-area-inset-left))] pt-2 pb-6 backdrop-blur-lg"
      >
        <ul className="flex flex-col">
          {[...siteConfig.nav, siteConfig.signIn].map((link) => (
            <li key={link.label}>
              <a
                href={link.href}
                {...("external" in link && link.external
                  ? { target: "_blank", rel: "noopener noreferrer" }
                  : {})}
                onClick={() => setOpen(false)}
                className="flex min-h-12 items-center border-b border-paper/10 text-lg text-paper-muted hover:text-white"
              >
                {link.label}
              </a>
            </li>
          ))}
        </ul>
      </nav>
    </div>
  );
}
