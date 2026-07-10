import { useKeyboard, useRenderer, useSelectionHandler } from "@opentui/react";
import { useEffect, useRef, useState } from "react";
import type { Feedback } from "./CommandBar.tsx";

const QUIT_CONFIRM_WINDOW_MS = 2_000;

/**
 * Comportement façon Claude Code : surligner du texte le copie directement
 * (OSC 52, marche même sans clipboard système local — le terminal s'en charge).
 * Ctrl+C ne quitte pas au premier coup : il faut confirmer dans les 2s, sinon
 * il est réarmé. Nécessite `exitOnCtrlC: false` sur le renderer.
 */
export function useTerminalShortcuts(setFeedback: (feedback: Feedback) => void): void {
  const renderer = useRenderer();
  const quitArmedRef = useRef(false);
  const quitTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useSelectionHandler((selection) => {
    const text = selection.getSelectedText();
    if (!text) return;
    renderer.copyToClipboardOSC52(text);
    setFeedback({ kind: "success", message: `copié (${text.length} caractères)` });
  });

  useKeyboard((key) => {
    if (key.name !== "c" || !key.ctrl) return;

    if (quitArmedRef.current) {
      renderer.destroy();
      process.exit(0);
      return;
    }

    quitArmedRef.current = true;
    setFeedback({ kind: "info", message: "Ctrl+C à nouveau pour quitter" });
    clearTimeout(quitTimerRef.current);
    quitTimerRef.current = setTimeout(() => {
      quitArmedRef.current = false;
    }, QUIT_CONFIRM_WINDOW_MS);
  });
}

/** Horloge partagée (1 tick/s) pour l'heure du header, l'âge des positions, le temps relatif des news. */
export function useClock(): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1_000);
    return () => clearInterval(id);
  }, []);
  return now;
}

/** Exécute `callback` immédiatement puis toutes les `delayMs` ms, jusqu'au démontage. */
export function useInterval(callback: () => void, delayMs: number): void {
  const callbackRef = useRef(callback);
  callbackRef.current = callback;

  useEffect(() => {
    callbackRef.current();
    const id = setInterval(() => callbackRef.current(), delayMs);
    return () => clearInterval(id);
  }, [delayMs]);
}
