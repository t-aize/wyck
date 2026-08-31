import { Effect } from "effect";
import { useEffect, useMemo, useState } from "react";
import type { StructureReading } from "../../structure/bias.ts";
import type { StructurePeriod } from "../../structure/fetch.ts";
import { fetchStructure } from "../../structure/fetch.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useInterval } from "./useInterval.ts";

/** Un swing M1/M5/M15/H1 ne bouge pas plus vite que ça — pas besoin du rythme 3s de useMarketData.ts. */
const STRUCTURE_POLL_MS = 60_000;

interface Structure {
  structure: Record<StructurePeriod, StructureReading> | undefined;
  structureError: string | undefined;
}

/** `client`/`symbolId` viennent de `useCtrader()`, même pattern que useMarketData.ts — purement
 * informatif, aucune commande ne lit ce hook. */
export function useStructure(): Structure {
  const { client, symbolId } = useCtrader();
  const [structure, setStructure] = useState<Record<StructurePeriod, StructureReading>>();
  const [structureError, setStructureError] = useState<string>();

  const refreshStructure = useMemo(
    () => async () => {
      if (!symbolId) return;
      try {
        setStructure(await Effect.runPromise(fetchStructure(client, symbolId)));
        setStructureError(undefined);
      } catch (error) {
        setStructureError(toMessage(error));
      }
    },
    [client, symbolId],
  );

  useInterval(refreshStructure, STRUCTURE_POLL_MS);

  // Même raison que useMarketData.ts : le premier tick de useInterval survient avant que symbolId
  // soit connu (connect() est async) — sans ce second effet, la structure n'apparaîtrait qu'au
  // prochain tick (jusqu'à STRUCTURE_POLL_MS de retard) après la connexion.
  useEffect(() => {
    if (symbolId) void refreshStructure();
  }, [symbolId, refreshStructure]);

  return { structure, structureError };
}
