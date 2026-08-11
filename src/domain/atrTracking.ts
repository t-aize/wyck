/**
 * Logique pure du suivi ATR (cf. ui/hooks/useAtrOrderTracking.ts pour le câblage React/Effect
 * autour de ces fonctions). Un ordre créé en mode ATR (cf. domain/trading.ts#prepareAtrTrade) reste
 * "suivi" tant qu'il est en attente : son SL/TP est recalculé à chaque nouvel ATR M5 et réamendé si
 * le résultat diffère de ce que le serveur rapporte actuellement.
 *
 * `CreateOrderResultSchema` (cf. ctrader/schemas.ts) est un payload non vérifié — aucun `orderId`
 * fiable en retour de `createOrder`. Un ordre fraîchement envoyé est donc mis en attente de
 * correspondance (`PendingAtrRegistration`) jusqu'à apparaître dans `positions.orders` (qui se
 * rafraîchit déjà toutes les 3s via useMarketData), identifié par ses caractéristiques plutôt que
 * par un id qu'on n'a pas.
 */

import type { CtraderOrder, OrderType, TradeSide } from "../ctrader/client.ts";
import { computeAtrLevels } from "./trading.ts";

export interface PendingAtrRegistration {
  symbolId: number;
  side: TradeSide;
  orderType: OrderType;
  volume: number;
  /** Prix d'entrée demandé (limitPrice ou stopPrice selon orderType) — clé de correspondance avec
   * l'ordre qui apparaît dans positions.orders après envoi. */
  price: number;
  atrMultiplier: number;
  rewardRiskRatio: number;
  /** epoch ms à la création de la registration — sert à la purger si jamais matchée. */
  queuedAt: number;
}

/** Réglages figés à la création de l'ordre (pas relus depuis les réglages globaux à chaque
 * réamend — changer `atr rr` ne doit pas modifier rétroactivement un ordre déjà en attente). */
export interface AtrTrackedOrder {
  side: TradeSide;
  entryPrice: number;
  atrMultiplier: number;
  rewardRiskRatio: number;
}

/** Trouve, parmi `orders`, celui qui correspond à `registration` et n'est pas déjà suivi —
 * `undefined` si l'ordre n'est pas encore apparu (pas encore traité côté serveur/pas encore repollé). */
export function matchPendingRegistration(
  registration: PendingAtrRegistration,
  orders: CtraderOrder[],
  alreadyTrackedIds: ReadonlySet<number>,
): CtraderOrder | undefined {
  return orders.find(
    (o) =>
      !alreadyTrackedIds.has(o.orderId) &&
      o.symbolId === registration.symbolId &&
      o.tradeSide === registration.side &&
      o.orderType === registration.orderType &&
      o.volume === registration.volume &&
      (o.limitPrice ?? o.stopPrice) === registration.price,
  );
}

export interface AtrAmendment {
  orderId: number;
  stopLoss: number;
  takeProfit: number;
}

/**
 * Pour chaque ordre suivi encore présent dans `orders`, recalcule SL/TP depuis `atrValue` (prix
 * affiché, pas l'échelle brute x10^5 — cf. commentaire de tête de trading.ts) et ne retient que ceux
 * dont le résultat diffère du SL/TP actuellement rapporté par le serveur. La source de vérité est
 * `order.stopLoss`/`order.takeProfit` (pas un cache local) : après un amend réussi, le prochain poll
 * ramène la valeur à jour et cette fonction cesse naturellement de le ré-amender.
 */
export function computeAtrAmendments(
  tracked: ReadonlyMap<number, AtrTrackedOrder>,
  orders: CtraderOrder[],
  atrValue: number,
): AtrAmendment[] {
  const amendments: AtrAmendment[] = [];
  for (const [orderId, meta] of tracked) {
    const order = orders.find((o) => o.orderId === orderId);
    if (!order) continue; // rempli/annulé — pruné côté hook, pas ici.

    const { stopLoss, takeProfit } = computeAtrLevels(
      meta.side,
      meta.entryPrice,
      atrValue,
      meta.atrMultiplier,
      meta.rewardRiskRatio,
    );
    if (stopLoss !== order.stopLoss || takeProfit !== order.takeProfit) {
      amendments.push({ orderId, stopLoss, takeProfit });
    }
  }
  return amendments;
}
