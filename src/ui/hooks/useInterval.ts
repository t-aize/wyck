import { useEffect, useRef } from "react";

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
