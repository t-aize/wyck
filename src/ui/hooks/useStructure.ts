import { useEffect, useMemo, useState } from "react";
import type { CtraderClient, CtraderTrendbar, GetTrendbarsParams } from "../../ctrader/client.ts";
import {
  computeStructure,
  dropFormingBar,
  STRUCTURE_TIMEFRAMES,
  type StructureSnapshot,
} from "../../domain/structure.ts";
import { toMessage } from "../../errors.ts";
import { useInterval } from "./useInterval.ts";

/** La structure de marché évolue lentement (bougies 15M au plus fin) — inutile de poller au rythme du prix. */
const STRUCTURE_POLL_MS = 60_000;

/**
 * get_trendbars refuse toute plage > 720h et, en-dessous, ne renvoie que les bougies
 * réellement tradées (marché fermé le week-end) — pas le compte "attendu" pour la
 * plage demandée. Avec une seule fenêtre de 720h, le vrai swing low/high en 4H
 * (length=20) tombe parfois trop près du bord gauche des données pour être confirmé
 * (repro : 720h → 100 bougies 4H, le plus bas était à l'index 18 < length). Deux
 * fenêtres de 700h chaînées (sous le plafond serveur, cf. message d'erreur qui
 * suggère explicitement des appels multiples en parallèle) donnent assez de marge.
 */
const WINDOW_MS = 700 * 60 * 60_000;

async function fetchWindow(
  client: CtraderClient,
  symbolId: number,
  period: GetTrendbarsParams["period"],
  fromMs: number,
  toMs: number,
): Promise<CtraderTrendbar[]> {
  const { trendbars } = await client.getTrendbars({
    symbolId,
    period,
    fromTimestamp: new Date(fromMs).toISOString(),
    toTimestamp: new Date(toMs).toISOString(),
  });
  return trendbars;
}

export interface StructureRow {
  label: string;
  snapshot: StructureSnapshot;
}

export interface Structure {
  rows: StructureRow[] | undefined;
  structureError: string | undefined;
  refreshStructure: () => Promise<void>;
}

export function useStructure(client: CtraderClient, symbolId: number | undefined): Structure {
  const [rows, setRows] = useState<StructureRow[]>();
  const [structureError, setStructureError] = useState<string>();

  const refreshStructure = useMemo(
    () => async () => {
      if (!symbolId) return;
      try {
        const now = Date.now();
        const newerFrom = now - WINDOW_MS;
        const results = await Promise.all(
          STRUCTURE_TIMEFRAMES.map(async (tf) => {
            const [older, newer] = await Promise.all([
              fetchWindow(client, symbolId, tf.period, newerFrom - WINDOW_MS, newerFrom),
              fetchWindow(client, symbolId, tf.period, newerFrom, now),
            ]);
            const closed = dropFormingBar([...older, ...newer], tf.periodMs, now);
            return { label: tf.label, snapshot: computeStructure(closed, tf.length) };
          }),
        );
        setRows(results);
        setStructureError(undefined);
      } catch (error) {
        setStructureError(toMessage(error));
      }
    },
    [client, symbolId],
  );

  useInterval(refreshStructure, STRUCTURE_POLL_MS);

  // Même raison que useMarketData : le premier tick de useInterval tombe avant que
  // symbolId soit connu (connect() est async), sans quoi la structure resterait
  // vide jusqu'à STRUCTURE_POLL_MS après la connexion.
  useEffect(() => {
    if (symbolId) void refreshStructure();
  }, [symbolId, refreshStructure]);

  return { rows, structureError, refreshStructure };
}
