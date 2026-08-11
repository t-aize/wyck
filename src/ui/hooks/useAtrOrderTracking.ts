import { Effect } from "effect";
import { useEffect, useRef, useState } from "react";
import { PRICE_SCALE } from "../../constants.ts";
import type { CtraderOrder } from "../../ctrader/client.ts";
import {
  type AtrTrackedOrder,
  computeAtrAmendments,
  matchPendingRegistration,
  type PendingAtrRegistration,
} from "../../domain/atrTracking.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";

/** Une registration jamais matchée au-delà de ce délai est abandonnée (ordre rejeté côté serveur, ou
 * correspondance ratée) — feedback d'erreur plutôt qu'une attente indéfinie silencieuse. */
const REGISTRATION_TIMEOUT_MS = 30_000;

export interface AtrOrderTracking {
  /** Pour le badge visuel dans PositionsPanel — miroir React de registryRef, pas la source de vérité
   * (les effets ci-dessous lisent toujours registryRef.current directement). */
  trackedOrderIds: Set<number>;
  /** Appelé juste après un `createOrder` réussi en mode ATR (cf. useOrderActions.ts) — met en file
   * d'attente le suivi, le temps que l'ordre apparaisse dans `pendingOrders`. */
  registerPendingAtrOrder: (registration: Omit<PendingAtrRegistration, "queuedAt">) => void;
  /** Un `modify` manuel sur un ordre suivi désactive son suivi ATR — l'intervention manuelle prime. */
  untrackOrder: (orderId: number) => void;
}

/**
 * Système de suivi du mode ATR (cf. domain/trading.ts#prepareAtrTrade, domain/atrTracking.ts pour la
 * logique pure) : tant qu'un ordre créé en mode ATR reste en attente, son SL/TP est recalculé à
 * chaque nouveau tick d'ATR et réamendé si le résultat diffère de ce que le serveur rapporte.
 * `atrRaw` reflète toujours le réglage *courant* (`atr period`/`atr timeframe`, cf. commands.ts),
 * pas celui en vigueur à la création de l'ordre — voulu, cf. commentaire équivalent sur
 * PreparedTrade.atrTracking dans domain/trading.ts. S'appuie entièrement sur le polling déjà en
 * place ailleurs (`pendingOrders` toutes les 3s via useMarketData, `atrRaw` toutes les 60s via
 * useTrend) — pas de nouvel intervalle ici.
 */
export function useAtrOrderTracking(opts: {
  pendingOrders: CtraderOrder[];
  /** ATR le plus récent sur le timeframe configuré, échelle brute x10^5 (cf. useTrend.ts#atr). */
  atrRaw: number | undefined;
}): AtrOrderTracking {
  const { pendingOrders, atrRaw } = opts;
  const { client } = useCtrader();
  const { setFeedback } = useFeedback();

  const registryRef = useRef(new Map<number, AtrTrackedOrder>());
  const pendingQueueRef = useRef<PendingAtrRegistration[]>([]);
  // Évite de ré-amender en boucle sur les polls (3s) qui précèdent la confirmation serveur du
  // dernier amend — sans ça, `computeAtrAmendments` continuerait de proposer le même changement
  // tant que `order.stopLoss` (source de vérité) n'a pas encore été rafraîchi côté serveur.
  const lastAppliedRef = useRef(new Map<number, { stopLoss: number; takeProfit: number }>());
  const [trackedOrderIds, setTrackedOrderIds] = useState<Set<number>>(new Set());

  function registerPendingAtrOrder(registration: Omit<PendingAtrRegistration, "queuedAt">) {
    pendingQueueRef.current = [
      ...pendingQueueRef.current,
      { ...registration, queuedAt: Date.now() },
    ];
  }

  function untrackOrder(orderId: number) {
    if (!registryRef.current.delete(orderId)) return;
    lastAppliedRef.current.delete(orderId);
    setTrackedOrderIds((ids) => {
      if (!ids.has(orderId)) return ids;
      const next = new Set(ids);
      next.delete(orderId);
      return next;
    });
  }

  // Correspondance des registrations en attente + purge des ordres suivis qui ont disparu (remplis
  // ou annulés).
  useEffect(() => {
    if (pendingQueueRef.current.length > 0) {
      const now = Date.now();
      const stillPending: PendingAtrRegistration[] = [];
      for (const registration of pendingQueueRef.current) {
        const match = matchPendingRegistration(
          registration,
          pendingOrders,
          new Set(registryRef.current.keys()),
        );
        if (match) {
          registryRef.current.set(match.orderId, {
            side: registration.side,
            entryPrice: registration.price,
            atrMultiplier: registration.atrMultiplier,
            rewardRiskRatio: registration.rewardRiskRatio,
          });
          setTrackedOrderIds((ids) => new Set(ids).add(match.orderId));
        } else if (now - registration.queuedAt > REGISTRATION_TIMEOUT_MS) {
          setFeedback({
            kind: "error",
            message:
              `suivi ATR non activé pour l'ordre ${registration.side} ${registration.price} — ` +
              "vérifie/ajuste-le manuellement",
          });
        } else {
          stillPending.push(registration);
        }
      }
      pendingQueueRef.current = stillPending;
    }

    const stillOpenIds = new Set(pendingOrders.map((o) => o.orderId));
    const toDrop = [...registryRef.current.keys()].filter((id) => !stillOpenIds.has(id));
    if (toDrop.length > 0) {
      for (const id of toDrop) {
        registryRef.current.delete(id);
        lastAppliedRef.current.delete(id);
      }
      setTrackedOrderIds((ids) => {
        const next = new Set(ids);
        for (const id of toDrop) next.delete(id);
        return next;
      });
    }
  }, [pendingOrders, setFeedback]);

  // Réamende les ordres suivis dont le SL/TP calculé diverge de ce que le serveur rapporte —
  // déclenché par un nouveau tick ATR (toutes les 60s) ou par `pendingOrders` (capte un ordre tout
  // juste matché ci-dessus sans attendre le prochain tick ATR).
  useEffect(() => {
    if (atrRaw === undefined || registryRef.current.size === 0) return;

    const atrValue = atrRaw / PRICE_SCALE;
    const amendments = computeAtrAmendments(registryRef.current, pendingOrders, atrValue).filter(
      (a) => {
        const applied = lastAppliedRef.current.get(a.orderId);
        return !applied || applied.stopLoss !== a.stopLoss || applied.takeProfit !== a.takeProfit;
      },
    );
    if (amendments.length === 0) return;

    for (const a of amendments) {
      lastAppliedRef.current.set(a.orderId, { stopLoss: a.stopLoss, takeProfit: a.takeProfit });
    }

    // Même idiome que confirmPendingCancel dans useOrderActions.ts : chaque résultat porte
    // directement son orderId, tolérant à un échec individuel sans faire échouer les autres amends.
    const amendAll = Effect.forEach(
      amendments,
      (a) => {
        const order = pendingOrders.find((o) => o.orderId === a.orderId);
        return client
          .amendOrder({
            orderId: a.orderId,
            limitPrice: order?.limitPrice,
            stopPrice: order?.stopPrice,
            stopLoss: a.stopLoss,
            takeProfit: a.takeProfit,
          })
          .pipe(
            Effect.as({ orderId: a.orderId, ok: true as const }),
            Effect.catchAll(() => Effect.succeed({ orderId: a.orderId, ok: false as const })),
          );
      },
      { concurrency: "unbounded" },
    );

    void Effect.runPromise(amendAll).then((results) => {
      const ok = results.filter((r) => r.ok).map((r) => r.orderId);
      const failed = results.filter((r) => !r.ok).map((r) => r.orderId);
      if (ok.length > 0) {
        setFeedback({
          kind: "success",
          message: `SL/TP ATR ajustés : ordre${ok.length > 1 ? "s" : ""} ${ok.join(", ")}`,
        });
      }
      if (failed.length > 0) {
        setFeedback({
          kind: "error",
          message: `échec ajustement ATR : ordre${failed.length > 1 ? "s" : ""} ${failed.join(", ")}`,
        });
      }
    });
  }, [atrRaw, pendingOrders, client, setFeedback]);

  return { trackedOrderIds, registerPendingAtrOrder, untrackOrder };
}
