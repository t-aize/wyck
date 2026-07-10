import { useEffect, useMemo, useState } from "react";
import type { CtraderClient, GetPositionsResult } from "../../ctrader/client.ts";
import { toMessage } from "../../errors.ts";
import { useInterval } from "./useInterval.ts";

const PRICE_POLL_MS = 3_000;

export interface MarketData {
  bid: number | undefined;
  ask: number | undefined;
  positions: GetPositionsResult | undefined;
  refreshMarket: () => Promise<void>;
}

/** `onError` partage l'état `connectionError` de useCtraderConnection (même affichage). */
export function useMarketData(
  client: CtraderClient,
  symbolId: number | undefined,
  onError: (message: string | undefined) => void,
): MarketData {
  const [bid, setBid] = useState<number>();
  const [ask, setAsk] = useState<number>();
  const [positions, setPositions] = useState<GetPositionsResult>();

  const refreshMarket = useMemo(
    () => async () => {
      if (!symbolId) return;
      try {
        const [spot, pos] = await Promise.all([
          client.getSpotPrices({ symbolId: [symbolId] }),
          client.getPositions(),
        ]);
        const price = spot.prices[0];
        if (price) {
          setBid(price.bid);
          setAsk(price.ask);
        }
        setPositions(pos);
        onError(undefined);
      } catch (error) {
        onError(toMessage(error));
      }
    },
    [client, symbolId, onError],
  );

  useInterval(refreshMarket, PRICE_POLL_MS);

  // `useInterval` déclenche son premier appel immédiat au montage, donc avant que `symbolId`
  // soit connu (connect() est async) — ce premier appel n'a aucun effet. Sans ce second effet,
  // positions/ordres n'apparaissent qu'au prochain tick (jusqu'à PRICE_POLL_MS de retard) après
  // la connexion, au lieu d'être visibles immédiatement.
  useEffect(() => {
    if (symbolId) void refreshMarket();
  }, [symbolId, refreshMarket]);

  return { bid, ask, positions, refreshMarket };
}
