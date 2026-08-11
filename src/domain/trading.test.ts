import { describe, expect, test } from "bun:test";
import { Effect, Layer } from "effect";
import { PRICE_SCALE } from "../constants.ts";
import { CtraderClient, type CtraderClientLive } from "../ctrader/client.ts";
import {
  type AtrTradeInput,
  computeAtrLevels,
  computeUnrealizedPnl,
  conflictsWithHtfBias,
  prepareAtrTrade,
  prepareTrade,
  type TradeInput,
  toCreateOrderParams,
} from "./trading.ts";

/** Simule uniquement les deux méthodes que `prepareTrade` appelle — pas besoin d'un vrai client MCP. */
function fakeClient(opts: {
  bid: number;
  ask: number;
  equity: number;
  moneyDigits: number;
}): CtraderClientLive {
  return {
    getSpotPrices: () =>
      Effect.succeed({
        prices: [
          {
            symbolId: 1,
            bid: opts.bid * PRICE_SCALE,
            ask: opts.ask * PRICE_SCALE,
            high: 0,
            low: 0,
            sessionClose: 0,
            timestamp: 0,
          },
        ],
      }),
    getBalance: () =>
      Effect.succeed({
        balance: 0,
        equity: opts.equity,
        freeMargin: 0,
        balanceVersion: 0,
        moneyDigits: opts.moneyDigits,
        depositAssetId: 0,
      }),
  } as unknown as CtraderClientLive;
}

// equity=1_000_000 à moneyDigits=2 → 10 000 $ de compte.
const client = fakeClient({ bid: 4100.0, ask: 4100.2, equity: 1_000_000, moneyDigits: 2 });
const testLayer = Layer.succeed(CtraderClient, client);

/** `prepareTrade` reçoit désormais CtraderClient par injection (§4.1) plutôt qu'en paramètre —
 * ce helper fournit la Layer de test et exécute l'Effect, pour garder les tests ci-dessous
 * inchangés par ailleurs (même forme `await runPrepareTrade(...)` qu'un `await prepareTrade(...)`). */
function runPrepareTrade(symbolId: number, input: TradeInput) {
  return Effect.runPromise(Effect.provide(prepareTrade(symbolId, input), testLayer));
}

function runPrepareAtrTrade(
  symbolId: number,
  input: AtrTradeInput,
  atr: Parameters<typeof prepareAtrTrade>[2],
) {
  return Effect.runPromise(Effect.provide(prepareAtrTrade(symbolId, input, atr), testLayer));
}

describe("prepareTrade", () => {
  test("BUY market : direction déduite de SL<TP, entrée = ask", async () => {
    const input: TradeInput = { entry: "market", riskPercent: 1, stopLoss: 4090, takeProfit: 4110 };
    const trade = await runPrepareTrade(1, input);
    expect(trade.tradeSide).toBe("BUY");
    expect(trade.orderType).toBe("MARKET");
    expect(trade.entryPrice).toBeCloseTo(4100.2);
    expect(trade.volume % 100).toBe(0); // multiple du pas de 0.01 lot
    expect(trade.volumeLots).toBeCloseTo(0.1, 2);
    expect(trade.riskAmount).toBeCloseTo(100); // 1% de 10 000$
  });

  test("SELL market : direction déduite de SL>TP, entrée = bid", async () => {
    const input: TradeInput = { entry: "market", riskPercent: 1, stopLoss: 4110, takeProfit: 4090 };
    const trade = await runPrepareTrade(1, input);
    expect(trade.tradeSide).toBe("SELL");
    expect(trade.orderType).toBe("MARKET");
    expect(trade.entryPrice).toBeCloseTo(4100.0);
  });

  test("BUY avec entrée au-dessus de l'ask → STOP", async () => {
    const input: TradeInput = { entry: 4110, riskPercent: 1, stopLoss: 4090, takeProfit: 4130 };
    const trade = await runPrepareTrade(1, input);
    expect(trade.tradeSide).toBe("BUY");
    expect(trade.orderType).toBe("STOP");
  });

  test("BUY avec entrée sous l'ask → LIMIT", async () => {
    const input: TradeInput = { entry: 4095, riskPercent: 1, stopLoss: 4090, takeProfit: 4110 };
    const trade = await runPrepareTrade(1, input);
    expect(trade.tradeSide).toBe("BUY");
    expect(trade.orderType).toBe("LIMIT");
  });

  test("SELL avec entrée au-dessus du bid → LIMIT", async () => {
    const input: TradeInput = { entry: 4110, riskPercent: 1, stopLoss: 4130, takeProfit: 4090 };
    const trade = await runPrepareTrade(1, input);
    expect(trade.tradeSide).toBe("SELL");
    expect(trade.orderType).toBe("LIMIT");
  });

  test("SELL avec entrée sous le bid → STOP", async () => {
    const input: TradeInput = { entry: 4090, riskPercent: 1, stopLoss: 4130, takeProfit: 4070 };
    const trade = await runPrepareTrade(1, input);
    expect(trade.tradeSide).toBe("SELL");
    expect(trade.orderType).toBe("STOP");
  });

  test("rejette un risque hors de ]0, 100]", async () => {
    const bad: TradeInput = { entry: "market", riskPercent: 0, stopLoss: 4090, takeProfit: 4110 };
    await expect(runPrepareTrade(1, bad)).rejects.toThrow("Risque invalide");
    const bad2: TradeInput = {
      entry: "market",
      riskPercent: 101,
      stopLoss: 4090,
      takeProfit: 4110,
    };
    await expect(runPrepareTrade(1, bad2)).rejects.toThrow("Risque invalide");
  });

  test("rejette SL === TP", async () => {
    const bad: TradeInput = { entry: "market", riskPercent: 1, stopLoss: 4100, takeProfit: 4100 };
    await expect(runPrepareTrade(1, bad)).rejects.toThrow("identiques");
  });

  test("rejette un SL/TP incohérent pour un achat", async () => {
    // SL(4090) < TP(4110) ⇒ BUY déduit, mais TP(4110) sous l'entrée market(4100.2) est incohérent
    const bad: TradeInput = { entry: 4105, riskPercent: 1, stopLoss: 4090, takeProfit: 4100 };
    await expect(runPrepareTrade(1, bad)).rejects.toThrow("achat");
  });

  test("rejette un SL/TP incohérent pour une vente", async () => {
    const bad: TradeInput = { entry: 4095, riskPercent: 1, stopLoss: 4110, takeProfit: 4100 };
    await expect(runPrepareTrade(1, bad)).rejects.toThrow("vente");
  });

  test("rejette un volume sous le minimum de compte (0.01 lot)", async () => {
    // Risque ridiculement petit sur un stop très large ⇒ volume arrondi à 0.
    const bad: TradeInput = {
      entry: "market",
      riskPercent: 0.001,
      stopLoss: 3000,
      takeProfit: 5000,
    };
    await expect(runPrepareTrade(1, bad)).rejects.toThrow("sous le minimum");
  });
});

describe("computeAtrLevels", () => {
  test("BUY : SL en dessous de l'entrée, TP au-dessus, distance TP = RR × distance SL", () => {
    expect(computeAtrLevels("BUY", 4100, 5, 1, 1.2)).toEqual({
      stopLoss: 4095,
      takeProfit: 4106,
    });
  });

  test("SELL : symétrique (SL au-dessus, TP en dessous)", () => {
    expect(computeAtrLevels("SELL", 4100, 5, 1, 1.2)).toEqual({
      stopLoss: 4105,
      takeProfit: 4094,
    });
  });

  test("multiplicateur ATR appliqué à la distance de stop avant le RR", () => {
    // distance SL = 4 × 1.5 = 6 ; distance TP = 6 × 2 = 12.
    expect(computeAtrLevels("BUY", 4100, 4, 1.5, 2)).toEqual({ stopLoss: 4094, takeProfit: 4112 });
  });
});

describe("prepareAtrTrade", () => {
  // atrValue affiché = 5 (rawValue = 5 × PRICE_SCALE) ; multiplicateur 1, RR 1.2, période 14,
  // timeframe M5 (défauts du mode ATR, cf. config.ts#DEFAULT_ATR_SETTINGS) — mêmes chiffres que les
  // tests computeAtrLevels ci-dessus.
  const atr = {
    rawValue: 5 * PRICE_SCALE,
    multiplier: 1,
    rewardRiskRatio: 1.2,
    period: 14,
    timeframe: "M5" as const,
  };

  test("BUY market : entrée = ask, SL/TP dérivés de l'ATR, direction donnée pas déduite", async () => {
    const input: AtrTradeInput = { entry: "market", riskPercent: 1, side: "BUY" };
    const trade = await runPrepareAtrTrade(1, input, atr);
    expect(trade.tradeSide).toBe("BUY");
    expect(trade.orderType).toBe("MARKET");
    expect(trade.entryPrice).toBeCloseTo(4100.2);
    expect(trade.stopLoss).toBeCloseTo(4095.2);
    expect(trade.takeProfit).toBeCloseTo(4106.2);
    expect(trade.atrTracking).toEqual({
      atrMultiplier: 1,
      rewardRiskRatio: 1.2,
      atrPeriod: 14,
      atrTimeframe: "M5",
    });
  });

  test("SELL market : entrée = bid", async () => {
    const input: AtrTradeInput = { entry: "market", riskPercent: 1, side: "SELL" };
    const trade = await runPrepareAtrTrade(1, input, atr);
    expect(trade.tradeSide).toBe("SELL");
    expect(trade.entryPrice).toBeCloseTo(4100.0);
    expect(trade.stopLoss).toBeCloseTo(4105.0);
    expect(trade.takeProfit).toBeCloseTo(4094.0);
  });

  test("BUY avec entrée fixe sous l'ask ⇒ LIMIT, SL/TP relatifs à cette entrée", async () => {
    const input: AtrTradeInput = { entry: 4095, riskPercent: 1, side: "BUY" };
    const trade = await runPrepareAtrTrade(1, input, atr);
    expect(trade.orderType).toBe("LIMIT");
    expect(trade.stopLoss).toBeCloseTo(4090);
    expect(trade.takeProfit).toBeCloseTo(4101);
  });

  test("ATR indisponible ⇒ AtrUnavailable plutôt qu'un calcul sur une valeur absente", async () => {
    const input: AtrTradeInput = { entry: "market", riskPercent: 1, side: "BUY" };
    await expect(runPrepareAtrTrade(1, input, { ...atr, rawValue: undefined })).rejects.toThrow(
      "ATR(14) M5",
    );
  });

  test("réutilise la même validation risque% que prepareTrade", async () => {
    const bad: AtrTradeInput = { entry: "market", riskPercent: 0, side: "BUY" };
    await expect(runPrepareAtrTrade(1, bad, atr)).rejects.toThrow("Risque invalide");
  });
});

describe("toCreateOrderParams", () => {
  test("ordre MARKET : SL/TP en points relatifs, pas en prix absolu", async () => {
    const trade = await runPrepareTrade(1, {
      entry: "market",
      riskPercent: 1,
      stopLoss: 4090,
      takeProfit: 4110,
    });
    const params = toCreateOrderParams(1, trade);
    expect(params.orderType).toBe("MARKET");
    expect(params.stopLoss).toBeUndefined();
    expect(params.takeProfit).toBeUndefined();
    expect(params.relativeStopLoss).toBe(
      Math.round(Math.abs(trade.entryPrice - 4090) * PRICE_SCALE),
    );
    expect(params.relativeTakeProfit).toBe(
      Math.round(Math.abs(trade.entryPrice - 4110) * PRICE_SCALE),
    );
  });

  test("ordre LIMIT : prix absolu, pas de points relatifs", async () => {
    const trade = await runPrepareTrade(1, {
      entry: 4095,
      riskPercent: 1,
      stopLoss: 4090,
      takeProfit: 4110,
    });
    const params = toCreateOrderParams(1, trade);
    expect(params.orderType).toBe("LIMIT");
    expect(params.limitPrice).toBe(4095);
    expect(params.stopPrice).toBeUndefined();
    expect(params.stopLoss).toBe(4090);
    expect(params.takeProfit).toBe(4110);
    expect(params.relativeStopLoss).toBeUndefined();
  });
});

describe("computeUnrealizedPnl", () => {
  test("BUY : marque au bid", () => {
    expect(computeUnrealizedPnl("BUY", 0.1, 4100, 4110, 4110.5)).toBeCloseTo(100);
  });

  test("SELL : marque à l'ask", () => {
    expect(computeUnrealizedPnl("SELL", 0.1, 4100, 4089.5, 4090)).toBeCloseTo(100);
  });

  test("perte quand le marché va contre la position", () => {
    expect(computeUnrealizedPnl("BUY", 0.1, 4100, 4090, 4090.5)).toBeCloseTo(-100);
  });
});

describe("conflictsWithHtfBias", () => {
  test("biais neutre (0, pas encore confirmé) ⇒ jamais de conflit", () => {
    expect(conflictsWithHtfBias("BUY", 0)).toBe(false);
    expect(conflictsWithHtfBias("SELL", 0)).toBe(false);
  });

  test("BUY aligné avec un biais haussier ⇒ pas de conflit", () => {
    expect(conflictsWithHtfBias("BUY", 1)).toBe(false);
  });

  test("BUY contre un biais baissier ⇒ conflit", () => {
    expect(conflictsWithHtfBias("BUY", -1)).toBe(true);
  });

  test("SELL aligné avec un biais baissier ⇒ pas de conflit", () => {
    expect(conflictsWithHtfBias("SELL", -1)).toBe(false);
  });

  test("SELL contre un biais haussier ⇒ conflit", () => {
    expect(conflictsWithHtfBias("SELL", 1)).toBe(true);
  });
});
