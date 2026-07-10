import { useKeyboard, useRenderer, useSelectionHandler } from "@opentui/react";
import { type Dispatch, type SetStateAction, useEffect, useRef, useState } from "react";
import type { Feedback } from "./CommandBar.tsx";

const QUIT_CONFIRM_WINDOW_MS = 2_000;
const COPY_FEEDBACK_MS = 3_000;

/**
 * Affiche `feedback`, puis restaure ce qu'il y avait avant après `durationMs` —
 * sauf si autre chose a déjà changé le message entretemps (mise à jour
 * fonctionnelle : on ne revient en arrière que si le message est encore le nôtre).
 */
function showTemporaryFeedback(
  setFeedback: Dispatch<SetStateAction<Feedback>>,
  feedback: Feedback,
  durationMs: number,
): void {
  let previous: Feedback | undefined;
  setFeedback((current) => {
    previous = current;
    return feedback;
  });
  setTimeout(() => {
    setFeedback((current) => (current === feedback ? (previous as Feedback) : current));
  }, durationMs);
}

/**
 * Comportement façon Claude Code : surligner du texte le copie directement
 * (OSC 52, marche même sans clipboard système local — le terminal s'en charge).
 * Ctrl+C ne quitte pas au premier coup : il faut confirmer dans les 2s, sinon
 * il est réarmé. Nécessite `exitOnCtrlC: false` sur le renderer.
 */
export function useTerminalShortcuts(setFeedback: Dispatch<SetStateAction<Feedback>>): void {
  const renderer = useRenderer();
  const quitArmedRef = useRef(false);
  const quitTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useSelectionHandler((selection) => {
    const text = selection.getSelectedText();
    if (!text) return;
    renderer.copyToClipboardOSC52(text);
    showTemporaryFeedback(
      setFeedback,
      { kind: "success", message: `copié (${text.length} caractères)` },
      COPY_FEEDBACK_MS,
    );
  });

  useKeyboard((key) => {
    if (key.name !== "c" || !key.ctrl) return;

    if (quitArmedRef.current) {
      renderer.destroy();
      process.exit(0);
      return;
    }

    quitArmedRef.current = true;
    showTemporaryFeedback(
      setFeedback,
      { kind: "info", message: "Ctrl+C à nouveau pour quitter" },
      QUIT_CONFIRM_WINDOW_MS,
    );
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
