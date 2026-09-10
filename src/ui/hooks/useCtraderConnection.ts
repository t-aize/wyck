import { Effect } from "effect";
import {
  type Dispatch,
  type SetStateAction,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { DEFAULT_SYMBOL } from "../../constants.ts";
import type { CtraderClient } from "../../ctrader/client.ts";
import { buildCatalog, findInstrument, type InstrumentSpecs } from "../../instrument/specs.ts";
import { writeConfig } from "../../settings.ts";
import { fsRuntime } from "../../utils/effectRuntime.ts";
import { toMessage } from "../../utils/errors.ts";

export interface CtraderConnection {
  connected: boolean;
  catalog: InstrumentSpecs[];
  instrument: InstrumentSpecs | undefined;
  connectionError: string | undefined;
  setConnectionError: Dispatch<SetStateAction<string | undefined>>;
  selectSymbol: (name: string) => boolean;
}

export function useCtraderConnection(
  client: CtraderClient,
  initialSymbol: string,
): CtraderConnection {
  const [connected, setConnected] = useState(false);
  const [connectionError, setConnectionError] = useState<string>();
  const [catalog, setCatalog] = useState<InstrumentSpecs[]>([]);
  const [selectedName, setSelectedName] = useState(initialSymbol);

  useEffect(() => {
    let cancelled = false;
    if (!client.isConfigured) return;

    void (async () => {
      try {
        await client.connect();
        const [{ symbols }, assetsResult] = await Promise.all([
          Effect.runPromise(client.getSymbols()),
          Effect.runPromise(client.getAssets()).then(
            (result) => result,
            () => ({ assets: [] }),
          ),
        ]);
        if (cancelled) return;
        const nextCatalog = buildCatalog(symbols, assetsResult.assets);
        setCatalog(nextCatalog);

        const requested = findInstrument(nextCatalog, initialSymbol);
        const fallback = requested ?? findInstrument(nextCatalog, DEFAULT_SYMBOL) ?? nextCatalog[0];
        if (!fallback) {
          throw new Error("Aucun symbole tradable renvoyé par le serveur");
        }
        if (!requested) {
          setSelectedName(fallback.symbolName);
          void fsRuntime.runPromise(writeConfig({ symbol: fallback.symbolName }));
        }
        setConnected(true);
      } catch (error) {
        if (!cancelled) setConnectionError(toMessage(error));
      }
    })();

    return () => {
      cancelled = true;
      void client.close();
    };
  }, [client, initialSymbol]);

  const instrument = useMemo(
    () => findInstrument(catalog, selectedName) ?? catalog[0],
    [catalog, selectedName],
  );

  const selectSymbol = useCallback(
    (name: string): boolean => {
      const found = findInstrument(catalog, name);
      if (!found) return false;
      setSelectedName(found.symbolName);
      void fsRuntime.runPromise(writeConfig({ symbol: found.symbolName }));
      return true;
    },
    [catalog],
  );

  return {
    connected,
    catalog,
    instrument,
    connectionError,
    setConnectionError,
    selectSymbol,
  };
}
