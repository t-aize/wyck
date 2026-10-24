"use client";

import { useEffect, useRef } from "react";
import { fragmentMono } from "~/app/fonts";
import { AsciiBackdrop as AsciiBackdropRenderer } from "~/lib/ascii/backdrop";

/** The hero's animated ASCII landscape. Decorative: hidden from assistive tech. */
export function AsciiBackdrop() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const root = canvas?.closest<HTMLElement>("[data-hero]");
    if (!(canvas && root)) {
      return;
    }
    const renderer = new AsciiBackdropRenderer({
      root,
      canvas,
      fontFamily: fragmentMono.style.fontFamily,
    });
    return () => renderer.destroy();
  }, []);

  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 z-0 overflow-hidden mask-[linear-gradient(to_bottom,black_85%,transparent)]"
    >
      <canvas
        ref={canvasRef}
        className="absolute top-0 left-0 opacity-0 transition-opacity duration-700 ease-out data-ready:opacity-100"
      />
      {/* Keeps the copy on plain ground: from the top on phones, from the left on wider screens. */}
      <div className="absolute inset-0 bg-[linear-gradient(to_bottom,rgb(17_18_14/0.92)_0%,rgb(17_18_14/0.7)_40%,transparent_68%)] md:bg-[linear-gradient(to_right,rgb(17_18_14/0.72)_0%,rgb(17_18_14/0.45)_34%,transparent_60%)]" />
    </div>
  );
}
