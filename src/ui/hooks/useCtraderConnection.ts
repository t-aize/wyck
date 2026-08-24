import { Effect } from "effect";
import { type Dispatch, type SetStateAction, useEffect, useState } from "react";
import { SYMBOL } from "../../constants.ts";
import type { CtraderClient } from "../../ctrader/client.ts";
import { toMessage } from "../../utils/errors.ts";

interface CtraderConnection {
  connected: boolean;
  symbolId: number | undefined;
  connectionError: string | undefined;
  /** Exposé pour que useMarketData reporte aussi ses erreurs dans le même état. */
  setConnectionError: Dispatch<SetStateAction<string | undefined>>;
}

export function useCtraderConnection(client: CtraderClient): CtraderConnection {
  const [connected, setConnected] = useState(false);
  const [connectionError, setConnectionError] = useState<string>();
  const [symbolId, setSymbolId] = useState<number>();

  useEffect(() => {
    let cancelled = false;
    // Pas encore configuré (url/token vides, cf. App.tsx#EMPTY_APP_CONFIG) : laisser
    // connected=false/connectionError=undefined plutôt que de tenter connect() (qui échouerait sur
    // `new URL("")` avec une erreur peu claire) — état neutre "pas encore connecté" jusqu'à ce que
    // `settings url`/`settings token` déclenchent un remount avec un client configuré.
    if (!client.isConfigured) return;

    void (async () => {
      try {
        await client.connect();
        const { symbols } = await Effect.runPromise(client.getSymbols());
        const symbol = symbols.find((s) => s.symbolName === SYMBOL);
        if (!symbol) throw new Error(`Symbole ${SYMBOL} introuvable côté serveur`);
        if (cancelled) return;
        setSymbolId(symbol.symbolId);
        setConnected(true);
      } catch (error) {
        if (!cancelled) setConnectionError(toMessage(error));
      }
    })();

    return () => {
      cancelled = true;
      void client.close();
    };
  }, [client]);

  return { connected, symbolId, connectionError, setConnectionError };
}
