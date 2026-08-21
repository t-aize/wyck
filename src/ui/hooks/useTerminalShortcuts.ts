import { useKeyboard, useRenderer, useSelectionHandler } from "@opentui/react";
import { type Dispatch, type SetStateAction, useRef } from "react";
import { useFeedback } from "../context/FeedbackContext.tsx";
import type { Feedback } from "../feedback.ts";

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
    setFeedback((current) => (current === feedback && previous !== undefined ? previous : current));
  }, durationMs);
}

/**
 * Comportement façon Claude Code : surligner du texte le copie directement
 * (OSC 52, marche même sans clipboard système local — le terminal s'en charge).
 * Ctrl+C ne quitte pas au premier coup : il faut confirmer dans les 2s, sinon
 * il est réarmé. Nécessite `exitOnCtrlC: false` sur le renderer.
 * Comportement façon shell : si la commande a du texte, Ctrl+C vide la ligne
 * au lieu d'armer la sortie (`tryClearInput` renvoie true si elle a effacé
 * quelque chose).
 */
export function useTerminalShortcuts(tryClearInput: () => boolean): void {
  const { setFeedback } = useFeedback();
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
    if (tryClearInput()) return;

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
