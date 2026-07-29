import { useEffect, useMemo, useRef, useState } from "react";
import type { CtraderClient, CtraderTrendbar, GetTrendbarsParams } from "../../ctrader/client.ts";
import { dropFormingBar } from "../../domain/smc/bars.ts";
import { computeStructure, type StructureRow } from "../../domain/smc/structure.ts";
import { STRUCTURE_TIMEFRAMES } from "../../domain/smc/timeframes.ts";
import { toMessage } from "../../errors.ts";
import { useInterval } from "./useInterval.ts";

/** La structure de marché évolue lentement (bougies 15M au plus fin) — inutile de poller au rythme du prix. */
const STRUCTURE_POLL_MS = 60_000;

/**
 * get_trendbars refuse toute plage > 720h et, en-dessous, ne renvoie que les bougies
 * réellement tradées (marché fermé le week-end) — pas le compte "attendu" pour la
 * plage demandée. Avec une seule fenêtre de 720h, le vrai swing low/high en 4H
 * (length=20) tombe parfois trop près du bord gauche des données pour être confirmé
 * (repro : 720h → 100 bougies 4H, le plus bas était à l'index 18 < length). Des
 * fenêtres de 700h chaînées (sous le plafond serveur, cf. message d'erreur qui
 * suggère explicitement des appels multiples en parallèle) donnent assez de marge —
 * autant de fenêtres que nécessaire pour couvrir `historyMs` de chaque timeframe
 * (2 pour la plupart, 10 pour D1 qui a besoin de bien plus d'historique).
 */
const REQUEST_CAP_MS = 700 * 60 * 60_000;

async function fetchHistory(
  client: CtraderClient,
  symbolId: number,
  period: GetTrendbarsParams["period"],
  historyMs: number,
  now: number,
): Promise<CtraderTrendbar[]> {
  const windowCount = Math.ceil(historyMs / REQUEST_CAP_MS);
  const windows = Array.from({ length: windowCount }, (_, i) => ({
    from: now - (i + 1) * REQUEST_CAP_MS,
    to: now - i * REQUEST_CAP_MS,
  })).reverse();

  const chunks = await Promise.all(
    windows.map(({ from, to }) =>
      client
        .getTrendbars({
          symbolId,
          period,
          fromTimestamp: new Date(from).toISOString(),
          toTimestamp: new Date(to).toISOString(),
        })
        .then((result) => result.trendbars),
    ),
  );
  return chunks.flat();
}

export type { StructureRow };

export interface Structure {
  rows: StructureRow[] | undefined;
  structureError: string | undefined;
  refreshStructure: (options?: { force?: boolean }) => Promise<void>;
}

export function useStructure(client: CtraderClient, symbolId: number | undefined): Structure {
  const [rows, setRows] = useState<StructureRow[]>();
  const [structureError, setStructureError] = useState<string>();
  // D1 (`dailyOnly`) ne change qu'une fois par jour et a un historique bien plus lourd à charger
  // (10 fenêtres chaînées) — on garde son dernier résultat en mémoire et on ne le refetch qu'une
  // fois par jour calendaire, plutôt qu'à chaque poll de structure (60s).
  const dailyCache = useRef(new Map<string, { dayKey: string; row: StructureRow }>());

  const refreshStructure = useMemo(
    () =>
      async (options: { force?: boolean } = {}) => {
        if (!symbolId) return;
        try {
          const now = Date.now();
          const today = new Date(now).toISOString().slice(0, 10);

          const results = await Promise.all(
            STRUCTURE_TIMEFRAMES.map(async (tf): Promise<StructureRow> => {
              if (tf.dailyOnly && !options.force) {
                const cached = dailyCache.current.get(tf.label);
                if (cached && cached.dayKey === today) return cached.row;
              }

              const raw = await fetchHistory(client, symbolId, tf.period, tf.historyMs, now);
              const closed = dropFormingBar(raw, tf.periodMs, now);
              const row: StructureRow = {
                label: tf.label,
                snapshot: computeStructure(closed, tf.length),
              };

              if (tf.dailyOnly) dailyCache.current.set(tf.label, { dayKey: today, row });
              return row;
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

  useInterval(() => void refreshStructure(), STRUCTURE_POLL_MS);

  // Même raison que useMarketData : le premier tick de useInterval tombe avant que
  // symbolId soit connu (connect() est async), sans quoi la structure resterait
  // vide jusqu'à STRUCTURE_POLL_MS après la connexion.
  useEffect(() => {
    if (symbolId) void refreshStructure();
  }, [symbolId, refreshStructure]);

  return { rows, structureError, refreshStructure };
}
