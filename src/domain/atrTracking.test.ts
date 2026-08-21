import { describe, expect, test } from "bun:test";
import type { CtraderOrder } from "../ctrader/schemas.ts";
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
    riskAmount: 50,
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
    riskAmount: 50,
  };
  // atrValue=5 ⇒ stopDistance=5 ⇒ SL=4095, TP=4106 (mêmes chiffres que computeAtrLevels dans
  // trading.test.ts) ⇒ volume=round((50/5*100)/100)*100=1000, inchangé par rapport à `order()`.

  test("détecte un changement quand le SL/TP serveur diverge du calcul", () => {
    const orders = [
      order({ orderId: 1, limitPrice: 4100, stopLoss: 4093, takeProfit: 4106, volume: 1000 }),
    ];
    const result = computeAtrAmendments(new Map([[1, tracked]]), orders, 5);
    expect(result).toEqual([{ orderId: 1, stopLoss: 4095, takeProfit: 4106, volume: 1000 }]);
  });

  test("aucun changement quand le SL/TP/volume serveur sont déjà à jour", () => {
    const orders = [
      order({ orderId: 1, limitPrice: 4100, stopLoss: 4095, takeProfit: 4106, volume: 1000 }),
    ];
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
      riskAmount: 50,
    };
    const orders = [
      order({ orderId: 1, limitPrice: 4100, stopLoss: 4095, takeProfit: 4106, volume: 1000 }), // BUY, à jour
      order({
        orderId: 2,
        tradeSide: "SELL",
        limitPrice: 4100,
        stopLoss: 4090,
        takeProfit: 4094,
        volume: 1000,
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
    expect(result).toEqual([{ orderId: 2, stopLoss: 4105, takeProfit: 4094, volume: 1000 }]);
  });

  test("recalcule le volume pour garder le risque$ constant quand l'ATR augmente (bug rapporté : sans ça le risque$ dérive avec l'ATR)", () => {
    // Ordre créé à atrValue=5 (SL=4095, TP=4106, volume=1000 pour risque$=50) — l'ATR passe à 10 :
    // stopDistance double, le volume doit être divisé par 2 pour garder le même risque$.
    const orders = [
      order({ orderId: 1, limitPrice: 4100, stopLoss: 4095, takeProfit: 4106, volume: 1000 }),
    ];
    const result = computeAtrAmendments(new Map([[1, tracked]]), orders, 10);
    expect(result).toEqual([{ orderId: 1, stopLoss: 4090, takeProfit: 4112, volume: 500 }]);
  });

  test("volume théorique sous le pas minimum ⇒ plancher à VOLUME_STEP (0.01 lot) plutôt que de bloquer l'amend", () => {
    const tinyRisk: AtrTrackedOrder = { ...tracked, riskAmount: 0.01 };
    const orders = [
      order({ orderId: 1, limitPrice: 4100, stopLoss: 4095, takeProfit: 4106, volume: 1000 }),
    ];
    const result = computeAtrAmendments(new Map([[1, tinyRisk]]), orders, 5);
    expect(result).toEqual([{ orderId: 1, stopLoss: 4095, takeProfit: 4106, volume: 100 }]);
  });
});
