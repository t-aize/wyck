import { ButtonLink } from "~/components/button-link";
import { siteConfig } from "~/config/site";
import { AsciiBackdrop } from "./ascii-backdrop";

const HEADLINE = ["Read", "the", "market,", "nothing", "else."];

// The copy sits in the sky on the left; the ASCII valley and the rising sun fill the rest.
export function Hero() {
  return (
    <section
      data-hero=""
      aria-labelledby="hero-title"
      className="relative isolate flex min-h-svh flex-col overflow-clip"
    >
      <AsciiBackdrop />

      <div className="relative z-10 mx-auto flex w-full max-w-300 flex-1 flex-col px-[max(1rem,env(safe-area-inset-left))] pt-[calc(4.75rem+clamp(2rem,10vh,7rem))] pb-[clamp(3rem,8vh,6rem)]">
        <div className="flex max-w-2xl flex-col items-center gap-5 self-center text-center md:items-start md:self-start md:text-left">
          <p
            className="animate-fade-up font-titling text-[0.75rem] tracking-[0.2em] text-balance text-paper-muted uppercase sm:text-[0.8125rem] sm:tracking-[0.26em]"
            style={{ animationDelay: "0.5s" }}
          >
            Trading terminal · for cTrader
          </p>
          <h1
            id="hero-title"
            className="m-0 max-w-[15ch] font-display text-[clamp(2.875rem,6.2vw,5.5rem)] leading-[1.02] font-normal tracking-[-0.012em] text-balance"
          >
            {HEADLINE.map((word, i) => (
              <span key={word}>
                <span className="reveal-word">
                  <span style={{ animationDelay: `${0.6 + i * 0.08}s` }}>{word}</span>
                </span>
                {i < HEADLINE.length - 1 ? " " : null}
              </span>
            ))}
          </h1>
          <p
            className="m-0 max-w-[46ch] animate-fade-up text-[clamp(1rem,1.4vw,1.15625rem)] leading-relaxed text-paper-muted"
            style={{ animationDelay: "1s" }}
          >
            {siteConfig.name} is a terminal for cTrader: rendered on the GPU, driven by the
            keyboard, quiet by design.
          </p>
          <div
            className="mt-1 flex animate-fade-up flex-wrap justify-center gap-3 md:justify-start"
            style={{ animationDelay: "1.15s" }}
          >
            <ButtonLink href={siteConfig.download.href} external={siteConfig.download.external}>
              Download {siteConfig.name}
              <svg
                className="arrow-drop"
                width="16"
                height="16"
                viewBox="0 0 16 16"
                aria-hidden="true"
              >
                <path
                  d="M8 2v9M4 7.5 8 11.5l4-4M3 14h10"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </ButtonLink>
            <ButtonLink href={siteConfig.signIn.href} variant="secondary">
              {siteConfig.signIn.label}
            </ButtonLink>
          </div>
          <p
            className="m-0 animate-fade-up font-mono text-xs text-paper-subtle"
            style={{ animationDelay: "1.3s" }}
          >
            Built with Rust and GPUI
          </p>
        </div>
      </div>
    </section>
  );
}
