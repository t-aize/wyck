import { useMemo, useState } from "react";
import { fetchMacro, type MacroSnapshot } from "../../domain/macro.ts";
import { toMessage } from "../../errors.ts";
import { useInterval } from "./useInterval.ts";

/** COT est hebdomadaire, dollar large/real yield quotidiens — pas besoin de poller plus souvent que le calendrier. */
const MACRO_POLL_MS = 30 * 60_000;

export interface Macro {
  macro: MacroSnapshot | undefined;
  macroError: string | undefined;
  refreshMacro: (options?: { force?: boolean }) => Promise<void>;
}

export function useMacroData(fredApiKey: string): Macro {
  const [macro, setMacro] = useState<MacroSnapshot>();
  const [macroError, setMacroError] = useState<string>();

  const refreshMacro = useMemo(
    () =>
      async (options: { force?: boolean } = {}) => {
        try {
          setMacro(await fetchMacro(fredApiKey, options));
          setMacroError(undefined);
        } catch (error) {
          setMacroError(toMessage(error));
        }
      },
    [fredApiKey],
  );

  useInterval(() => void refreshMacro(), MACRO_POLL_MS);

  return { macro, macroError, refreshMacro };
}
