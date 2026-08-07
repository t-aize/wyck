import { Effect } from "effect";
import { type Dispatch, type SetStateAction, useEffect, useState } from "react";
import { SYMBOL } from "../../constants.ts";
import type { CtraderClientLive } from "../../ctrader/client.ts";
import { toMessage } from "../../errors.ts";

export interface CtraderConnection {
  connected: boolean;
  symbolId: number | undefined;
  connectionError: string | undefined;
  /** Exposé pour que useMarketData reporte aussi ses erreurs dans le même état. */
  setConnectionError: Dispatch<SetStateAction<string | undefined>>;
}

export function useCtraderConnection(client: CtraderClientLive): CtraderConnection {
  const [connected, setConnected] = useState(false);
  const [connectionError, setConnectionError] = useState<string>();
  const [symbolId, setSymbolId] = useState<number>();

  useEffect(() => {
    let cancelled = false;

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
