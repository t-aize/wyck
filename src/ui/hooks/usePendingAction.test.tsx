import { describe, expect, test } from "bun:test";
import { Effect } from "effect";
import type { ReactElement } from "react";
import { act, create } from "react-test-renderer";
import { FeedbackProvider, useFeedback } from "../context/FeedbackContext.tsx";
import { type PendingAction, usePendingAction } from "./usePendingAction.ts";

// Requis depuis React 18 pour que act() s'exécute sans avertissement hors d'un environnement de
// test "connu" (Jest/RTL) — react-test-renderer n'active pas ce flag lui-même.
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Petit harnais de test : monte le hook sous <FeedbackProvider> (requis par usePendingAction, cf.
 * useFeedback()) et capture son retour + le feedback courant à chaque rendu, dans des variables
 * mutables lues après chaque `act()` — pas de rendu opentui (`<box>`/`<text>`) ici, seulement des
 * composants React ordinaires, donc `react-test-renderer` (pas besoin d'un vrai terminal). */
function renderPendingAction<T>(opts: Parameters<typeof usePendingAction<T>>[0]) {
  let action!: PendingAction<T>;
  let feedback!: { kind: string; message: string };

  function Harness() {
    action = usePendingAction<T>(opts);
    feedback = useFeedback().feedback;
    return null;
  }

  act(() => {
    // Cast : le jsxImportSource du projet ("@opentui/react", cf. tsconfig.json) type les éléments
    // JSX en `ReactNode` plutôt que `ReactElement` — sans incidence à l'exécution ici (aucun
    // intrinsèque opentui rendu), juste une différence de typage face à `create()` qui attend un
    // `ReactElement`.
    create(
      (
        <FeedbackProvider>
          <Harness />
        </FeedbackProvider>
      ) as ReactElement,
    );
  });

  return {
    get action() {
      return action;
    },
    get feedback() {
      return feedback;
    },
  };
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

/** Laisse les micro/macrotâches en vol (résolution de Promise, `.then()` d'Effect.runPromise, puis
 * le `setFeedback` React qui en découle) se terminer avant de lire l'état suivant. */
async function flush() {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("usePendingAction", () => {
  test("propose() sets pending and shows the propose message", () => {
    const view = renderPendingAction<string>({
      refreshMarket: async () => {},
      proposeMessage: "proposé",
      progressMessage: "en cours",
      cancelMessage: "annulé",
      errorPrefix: "échec",
      run: () => Effect.succeed(undefined),
      successMessage: (v) => `ok: ${v}`,
    });

    act(() => {
      view.action.propose("trade-1");
    });

    expect(view.action.pending).toBe("trade-1");
    expect(view.feedback).toEqual({ kind: "info", message: "proposé" });
  });

  test("cancel() clears pending and shows the cancel message", () => {
    const view = renderPendingAction<string>({
      refreshMarket: async () => {},
      proposeMessage: "proposé",
      progressMessage: "en cours",
      cancelMessage: "annulé",
      errorPrefix: "échec",
      run: () => Effect.succeed(undefined),
      successMessage: (v) => `ok: ${v}`,
    });

    act(() => {
      view.action.propose("trade-1");
    });
    act(() => {
      view.action.cancel();
    });

    expect(view.action.pending).toBeUndefined();
    expect(view.feedback).toEqual({ kind: "info", message: "annulé" });
  });

  test("confirm() with no pending value is a no-op", () => {
    const view = renderPendingAction<string>({
      refreshMarket: async () => {},
      proposeMessage: "proposé",
      progressMessage: "en cours",
      cancelMessage: "annulé",
      errorPrefix: "échec",
      run: () => Effect.succeed(undefined),
      successMessage: (v) => `ok: ${v}`,
    });

    act(() => {
      view.action.confirm();
    });

    expect(view.feedback.message).not.toBe("en cours");
  });

  test("confirm(): success clears pending, shows progress then success, and refreshes", async () => {
    const { effect, settle } = deferredEffect<void>();
    let refreshed = false;

    const view = renderPendingAction<string>({
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
      view.action.propose("trade-1");
    });
    act(() => {
      view.action.confirm();
    });

    // Pending is cleared and the "in progress" message shows immediately, before the Effect settles.
    expect(view.action.pending).toBeUndefined();
    expect(view.feedback).toEqual({ kind: "info", message: "en cours" });

    await act(async () => {
      settle({ ok: true, value: undefined });
      await flush();
    });

    expect(view.feedback).toEqual({ kind: "success", message: "ok: trade-1" });
    expect(refreshed).toBe(true);
  });

  test("confirm(): failure surfaces the error via errorPrefix, without refreshing", async () => {
    const { effect, settle } = deferredEffect<void>();
    let refreshed = false;

    const view = renderPendingAction<string>({
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
      view.action.propose("trade-1");
    });
    act(() => {
      view.action.confirm();
    });

    await act(async () => {
      settle({ ok: false, error: new Error("réseau indisponible") });
      await flush();
    });

    expect(view.feedback.kind).toBe("error");
    expect(view.feedback.message).toContain("échec envoi");
    expect(view.feedback.message).toContain("réseau indisponible");
    expect(refreshed).toBe(false);
  });
});
