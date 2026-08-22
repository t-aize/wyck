import { describe, expect, test } from "bun:test";
import { act, renderHook, waitFor } from "@testing-library/react";
import { Effect } from "effect";
import type { ReactNode } from "react";
import { FeedbackProvider, useFeedback } from "../context/FeedbackContext.tsx";
import { usePendingAction } from "./usePendingAction.ts";

/** Monte le hook sous <FeedbackProvider> (requis par usePendingAction, cf. useFeedback()) et
 * expose son retour + le feedback courant via `result.current` — rendu via react-dom
 * (@testing-library/react, cf. test/happydom.ts pour le DOM global), pas le renderer terminal
 * d'@opentui/react : ce harnais ne rend aucun intrinsèque opentui (`<box>`/`<text>`), donc les deux
 * mondes ne se croisent jamais ici. */
function setup<T>(opts: Parameters<typeof usePendingAction<T>>[0]) {
  return renderHook(
    () => {
      const action = usePendingAction<T>(opts);
      const { feedback } = useFeedback();
      return { action, feedback };
    },
    {
      wrapper: ({ children }: { children: ReactNode }) => (
        <FeedbackProvider>{children}</FeedbackProvider>
      ),
    },
  );
}

/** Une Effect contrôlée depuis le test : ne se résout/n'échoue que quand on appelle `settle()`, pour
 * pouvoir observer l'état "en cours" avant de laisser la Promise sous-jacente se terminer. */
function deferredEffect<A>() {
  let settle!: (result: { ok: true; value: A } | { ok: false; error: unknown }) => void;
  const promise = new Promise<A>((resolve, reject) => {
    settle = (result) => (result.ok ? resolve(result.value) : reject(result.error));
  });
  return { effect: Effect.promise(() => promise), settle };
}

describe("usePendingAction", () => {
  test("propose() sets pending and shows the propose message", () => {
    const { result } = setup<string>({
      refreshMarket: async () => {},
      proposeMessage: "proposé",
      progressMessage: "en cours",
      cancelMessage: "annulé",
      errorPrefix: "échec",
      run: () => Effect.succeed(undefined),
      successMessage: (v) => `ok: ${v}`,
    });

    act(() => {
      result.current.action.propose("trade-1");
    });

    expect(result.current.action.pending).toBe("trade-1");
    expect(result.current.feedback).toEqual({ kind: "info", message: "proposé" });
  });

  test("cancel() clears pending and shows the cancel message", () => {
    const { result } = setup<string>({
      refreshMarket: async () => {},
      proposeMessage: "proposé",
      progressMessage: "en cours",
      cancelMessage: "annulé",
      errorPrefix: "échec",
      run: () => Effect.succeed(undefined),
      successMessage: (v) => `ok: ${v}`,
    });

    act(() => {
      result.current.action.propose("trade-1");
    });
    act(() => {
      result.current.action.cancel();
    });

    expect(result.current.action.pending).toBeUndefined();
    expect(result.current.feedback).toEqual({ kind: "info", message: "annulé" });
  });

  test("confirm() with no pending value is a no-op", () => {
    const { result } = setup<string>({
      refreshMarket: async () => {},
      proposeMessage: "proposé",
      progressMessage: "en cours",
      cancelMessage: "annulé",
      errorPrefix: "échec",
      run: () => Effect.succeed(undefined),
      successMessage: (v) => `ok: ${v}`,
    });

    act(() => {
      result.current.action.confirm();
    });

    expect(result.current.feedback.message).not.toBe("en cours");
  });

  test("confirm(): success clears pending, shows progress then success, and refreshes", async () => {
    const { effect, settle } = deferredEffect<void>();
    let refreshed = false;

    const { result } = setup<string>({
      refreshMarket: async () => {
        refreshed = true;
      },
      proposeMessage: "proposé",
      progressMessage: "en cours",
      cancelMessage: "annulé",
      errorPrefix: "échec",
      run: () => effect,
      successMessage: (v) => `ok: ${v}`,
    });

    act(() => {
      result.current.action.propose("trade-1");
    });
    act(() => {
      result.current.action.confirm();
    });

    // Pending is cleared and the "in progress" message shows immediately, before the Effect settles.
    expect(result.current.action.pending).toBeUndefined();
    expect(result.current.feedback).toEqual({ kind: "info", message: "en cours" });

    act(() => {
      settle({ ok: true, value: undefined });
    });

    await waitFor(() => {
      expect(result.current.feedback).toEqual({ kind: "success", message: "ok: trade-1" });
    });
    expect(refreshed).toBe(true);
  });

  test("confirm(): failure surfaces the error via errorPrefix, without refreshing", async () => {
    const { effect, settle } = deferredEffect<void>();
    let refreshed = false;

    const { result } = setup<string>({
      refreshMarket: async () => {
        refreshed = true;
      },
      proposeMessage: "proposé",
      progressMessage: "en cours",
      cancelMessage: "annulé",
      errorPrefix: "échec envoi",
      run: () => effect,
      successMessage: (v) => `ok: ${v}`,
    });

    act(() => {
      result.current.action.propose("trade-1");
    });
    act(() => {
      result.current.action.confirm();
    });

    act(() => {
      settle({ ok: false, error: new Error("réseau indisponible") });
    });

    await waitFor(() => {
      expect(result.current.feedback.kind).toBe("error");
    });
    expect(result.current.feedback.message).toContain("échec envoi");
    expect(result.current.feedback.message).toContain("réseau indisponible");
    expect(refreshed).toBe(false);
  });
});
