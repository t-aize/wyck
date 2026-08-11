import { describe, expect, test } from "bun:test";
import type { CtraderOrder } from "../ctrader/client.ts";
import {
  type AtrTrackedOrder,
  computeAtrAmendments,
  matchPendingRegistration,
  type PendingAtrRegistration,
} from "./atrTracking.ts";

function order(overrides: Partial<CtraderOrder> & { orderId: number }): CtraderOrder {
  return {
    symbolId: 1,
    orderType: "LIMIT",
    tradeSide: "BUY",
    volume: 1000,
    ...overrides,
  };
}

function registration(overrides: Partial<PendingAtrRegistration> = {}): PendingAtrRegistration {
  return {
    symbolId: 1,
    side: "BUY",
    orderType: "LIMIT",
    volume: 1000,
    price: 4095,
    atrMultiplier: 1,
    rewardRiskRatio: 1.2,
    queuedAt: 0,
    ...overrides,
  };
}

describe("matchPendingRegistration", () => {
  test("trouve l'ordre correspondant (side/orderType/volume/prix), non déjà suivi", () => {
    const match = order({ orderId: 42, limitPrice: 4095 });
    const result = matchPendingRegistration(registration(), [match], new Set());
    expect(result?.orderId).toBe(42);
  });

  test("compare le prix via stopPrice pour un ordre STOP (pas limitPrice)", () => {
    const match = order({ orderId: 7, orderType: "STOP", stopPrice: 4110 });
    const reg = registration({ orderType: "STOP", price: 4110 });
    expect(matchPendingRegistration(reg, [match], new Set())?.orderId).toBe(7);
  });

  test("ignore un ordre déjà suivi même s'il correspond par ailleurs", () => {
    const match = order({ orderId: 42, limitPrice: 4095 });
    const result = matchPendingRegistration(registration(), [match], new Set([42]));
    expect(result).toBeUndefined();
  });

  test("aucune correspondance ⇒ undefined (side différent)", () => {
    const other = order({ orderId: 42, limitPrice: 4095, tradeSide: "SELL" });
    expect(matchPendingRegistration(registration(), [other], new Set())).toBeUndefined();
  });

  test("aucune correspondance ⇒ undefined (prix différent)", () => {
    const other = order({ orderId: 42, limitPrice: 4090 });
    expect(matchPendingRegistration(registration(), [other], new Set())).toBeUndefined();
  });

  test("liste vide ⇒ undefined (ordre pas encore apparu)", () => {
    expect(matchPendingRegistration(registration(), [], new Set())).toBeUndefined();
  });
});

describe("computeAtrAmendments", () => {
  const tracked: AtrTrackedOrder = {
    side: "BUY",
    entryPrice: 4100,
    atrMultiplier: 1,
    rewardRiskRatio: 1.2,
  };
  // atrValue=5 ⇒ SL=4095, TP=4106 (mêmes chiffres que computeAtrLevels dans trading.test.ts).

  test("détecte un changement quand le SL/TP serveur diverge du calcul", () => {
    const orders = [order({ orderId: 1, limitPrice: 4100, stopLoss: 4093, takeProfit: 4106 })];
    const result = computeAtrAmendments(new Map([[1, tracked]]), orders, 5);
    expect(result).toEqual([{ orderId: 1, stopLoss: 4095, takeProfit: 4106 }]);
  });

  test("aucun changement quand le SL/TP serveur est déjà à jour", () => {
    const orders = [order({ orderId: 1, limitPrice: 4100, stopLoss: 4095, takeProfit: 4106 })];
    expect(computeAtrAmendments(new Map([[1, tracked]]), orders, 5)).toEqual([]);
  });

  test("ordre suivi absent de la liste (rempli/annulé) ⇒ ignoré, pas d'erreur", () => {
    expect(computeAtrAmendments(new Map([[1, tracked]]), [], 5)).toEqual([]);
  });

  test("plusieurs ordres suivis, chacun évalué indépendamment", () => {
    const trackedSell: AtrTrackedOrder = {
      side: "SELL",
      entryPrice: 4100,
      atrMultiplier: 1,
      rewardRiskRatio: 1.2,
    };
    const orders = [
      order({ orderId: 1, limitPrice: 4100, stopLoss: 4095, takeProfit: 4106 }), // BUY, à jour
      order({
        orderId: 2,
        tradeSide: "SELL",
        limitPrice: 4100,
        stopLoss: 4090,
        takeProfit: 4094,
      }), // SELL, SL désynchronisé (devrait être 4105)
    ];
    const result = computeAtrAmendments(
      new Map([
        [1, tracked],
        [2, trackedSell],
      ]),
      orders,
      5,
    );
    expect(result).toEqual([{ orderId: 2, stopLoss: 4105, takeProfit: 4094 }]);
  });
});
