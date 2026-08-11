import { Effect, Fiber, Schedule } from "effect";
import { useEffect, useRef } from "react";

/**
 * Exécute `callback` immédiatement puis toutes les `delayMs` ms, jusqu'au démontage — piloté par
 * un fiber Effect (`Effect.repeat` + `Schedule.spaced`) plutôt que `setInterval`/`clearInterval`
 * (cf. docs/ARCHITECTURE.md §2.5/§3.3). Vérifié explicitement (pas juste supposé) que `Fiber.interrupt`
 * arrête bien la répétition au cleanup, sans tick fantôme après démontage.
 */
export function useInterval(callback: () => void, delayMs: number): void {
  const callbackRef = useRef(callback);
  callbackRef.current = callback;

  useEffect(() => {
    const tick = Effect.sync(() => callbackRef.current());
    const fiber = Effect.runFork(Effect.repeat(tick, Schedule.spaced(`${delayMs} millis`)));
    return () => {
      void Effect.runPromise(Fiber.interrupt(fiber));
    };
  }, [delayMs]);
}
