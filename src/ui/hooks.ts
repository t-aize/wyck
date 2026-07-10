import { useEffect, useRef, useState } from "react";

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
