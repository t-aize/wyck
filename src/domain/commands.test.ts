import { describe, expect, test } from "bun:test";
import type { CtraderOrder } from "../ctrader/client.ts";
import {
  ATR_SETTINGS_USAGE,
  ATR_TRADE_USAGE,
  CANCEL_USAGE,
  formatTradeSummary,
  MODIFY_USAGE,
  parseAtrSettingsCommand,
  parseAtrTradeCommand,
  parseModifyCommand,
  parseRiskCommand,
  parseTradeCommand,
  RISK_USAGE,
  resolveCancelTargets,
  TRADE_USAGE,
} from "./commands.ts";
import type { PreparedTrade } from "./trading.ts";

function order(orderId: number): CtraderOrder {
  return { orderId, symbolId: 1, orderType: "LIMIT", tradeSide: "BUY", volume: 10000 };
}

describe("parseTradeCommand", () => {
  test("parse risque/entrée/sl/tp valides", () => {
    const result = parseTradeCommand(["1", "4100", "4090", "4110"]);
    expect(result).toEqual({ entry: 4100, riskPercent: 1, stopLoss: 4090, takeProfit: 4110 });
  });

  test("accepte 'market' comme entrée", () => {
    const result = parseTradeCommand(["1", "market", "4090", "4110"]);
    expect(result).toEqual({ entry: "market", riskPercent: 1, stopLoss: 4090, takeProfit: 4110 });
  });

  test("arrondit les prix à 2 décimales (précision XAUUSD)", () => {
    const result = parseTradeCommand(["1", "4100.004", "4090.006", "4110.014"]);
    expect(result).toEqual({ entry: 4100, riskPercent: 1, stopLoss: 4090.01, takeProfit: 4110.01 });
  });

  test("signale les arguments manquants", () => {
    expect(parseTradeCommand(["1", "4100", "4090"])).toBe(`arguments manquants — ${TRADE_USAGE}`);
  });

  test("signale un risque non numérique", () => {
    expect(parseTradeCommand(["abc", "4100", "4090", "4110"])).toBe('risque invalide : "abc"');
  });

  test("signale une entrée non numérique (hors 'market')", () => {
    expect(parseTradeCommand(["1", "abc", "4090", "4110"])).toBe('entrée invalide : "abc"');
  });

  test("avec 3 arguments et un risque par défaut, interprète (entrée, sl, tp)", () => {
    const result = parseTradeCommand(["market", "4090", "4110"], 2);
    expect(result).toEqual({ entry: "market", riskPercent: 2, stopLoss: 4090, takeProfit: 4110 });
  });

  test("avec 3 arguments et aucun risque par défaut, réclame le risque%", () => {
    expect(parseTradeCommand(["market", "4090", "4110"])).toBe(
      `arguments manquants — ${TRADE_USAGE}`,
    );
  });

  test("avec 4 arguments, le risque par défaut est ignoré (celui donné prime)", () => {
    const result = parseTradeCommand(["3", "market", "4090", "4110"], 2);
    expect(result).toEqual({ entry: "market", riskPercent: 3, stopLoss: 4090, takeProfit: 4110 });
  });
});

describe("parseAtrTradeCommand", () => {
  test("parse risque/entrée/direction valides", () => {
    const result = parseAtrTradeCommand(["1", "4100", "buy"]);
    expect(result).toEqual({ entry: 4100, riskPercent: 1, side: "BUY" });
  });

  test("accepte 'market' comme entrée, direction insensible à la casse", () => {
    const result = parseAtrTradeCommand(["1", "market", "SELL"]);
    expect(result).toEqual({ entry: "market", riskPercent: 1, side: "SELL" });
  });

  test("avec 2 arguments et un risque par défaut, interprète (entrée, direction)", () => {
    const result = parseAtrTradeCommand(["market", "buy"], 2);
    expect(result).toEqual({ entry: "market", riskPercent: 2, side: "BUY" });
  });

  test("avec 2 arguments et aucun risque par défaut, réclame le risque%", () => {
    expect(parseAtrTradeCommand(["market", "buy"])).toBe(
      `arguments manquants — ${ATR_TRADE_USAGE}`,
    );
  });

  test("avec 3 arguments, le risque par défaut est ignoré (celui donné prime)", () => {
    const result = parseAtrTradeCommand(["3", "market", "buy"], 2);
    expect(result).toEqual({ entry: "market", riskPercent: 3, side: "BUY" });
  });

  test("signale une direction invalide", () => {
    expect(parseAtrTradeCommand(["1", "4100", "long"])).toBe(
      'direction invalide : "long" (buy/sell attendu)',
    );
  });

  test("signale une entrée non numérique (hors 'market')", () => {
    expect(parseAtrTradeCommand(["1", "abc", "buy"])).toBe('entrée invalide : "abc"');
  });

  test("signale un risque non numérique", () => {
    expect(parseAtrTradeCommand(["abc", "4100", "buy"])).toBe('risque invalide : "abc"');
  });
});

describe("parseModifyCommand", () => {
  test("parse --sl et --tp", () => {
    const result = parseModifyCommand(["42", "--sl", "4090", "--tp", "4110"]);
    expect(result).toEqual({ id: 42, stopLoss: 4090, takeProfit: 4110 });
  });

  test("accepte les alias courts -sl/-tp", () => {
    const result = parseModifyCommand(["42", "-sl", "4090"]);
    expect(result).toEqual({ id: 42, stopLoss: 4090, takeProfit: undefined });
  });

  test("signale un id invalide", () => {
    expect(parseModifyCommand(["abc", "--sl", "4090"])).toBe(
      `id invalide : "abc" — ${MODIFY_USAGE}`,
    );
  });

  test("exige au moins --sl ou --tp", () => {
    expect(parseModifyCommand(["42"])).toBe(`au moins --sl ou --tp requis — ${MODIFY_USAGE}`);
  });

  test("signale une option inconnue", () => {
    const result = parseModifyCommand(["42", "--bogus", "1"]);
    expect(typeof result).toBe("string");
    expect(result as string).toContain("option inconnue");
  });
});

describe("resolveCancelTargets", () => {
  test("cible les ordres en attente correspondant aux id donnés", () => {
    const orders = [order(1), order(2), order(3)];
    const result = resolveCancelTargets(["1", "3"], orders);
    expect(result).toEqual({ kind: "ids", orders: [order(1), order(3)] });
  });

  test("'all' cible tous les ordres en attente", () => {
    const orders = [order(1), order(2)];
    const result = resolveCancelTargets(["all"], orders);
    expect(result).toEqual({ kind: "all", orders });
  });

  test("'all' sans aucun ordre en attente renvoie un rejet de niveau info (pas une erreur de saisie)", () => {
    const result = resolveCancelTargets(["all"], []);
    expect(result).toEqual({
      kind: "rejected",
      level: "info",
      message: "aucun ordre en attente à annuler",
    });
  });

  test("sans argument, réclame l'usage", () => {
    expect(resolveCancelTargets([], [order(1)])).toEqual({
      kind: "rejected",
      level: "error",
      message: CANCEL_USAGE,
    });
  });

  test("signale un id non numérique", () => {
    const result = resolveCancelTargets(["abc"], [order(1)]);
    expect(result).toEqual({
      kind: "rejected",
      level: "error",
      message: 'id invalide : "abc"',
    });
  });

  test("signale un id qui ne correspond à aucun ordre en attente", () => {
    const result = resolveCancelTargets(["1", "99"], [order(1)]);
    expect(result).toEqual({
      kind: "rejected",
      level: "error",
      message: "ordre en attente 99 introuvable",
    });
  });
});

describe("parseRiskCommand", () => {
  test("parse un risque valide", () => {
    expect(parseRiskCommand(["2.5"])).toBe(2.5);
  });

  test("signale un risque manquant", () => {
    expect(parseRiskCommand([])).toBe(`risque manquant — ${RISK_USAGE}`);
  });

  test.each(["0", "101", "abc", "-1"])("rejette un risque hors bornes : %s", (raw) => {
    const result = parseRiskCommand([raw]);
    expect(typeof result).toBe("string");
    expect(result as string).toContain("risque invalide");
  });
});

describe("parseAtrSettingsCommand", () => {
  test("sans argument ⇒ {} (l'appelant affiche les réglages actuels)", () => {
    expect(parseAtrSettingsCommand([])).toEqual({});
  });

  test("atr rr <valeur>", () => {
    expect(parseAtrSettingsCommand(["rr", "1.5"])).toEqual({ rewardRiskRatio: 1.5 });
  });

  test("atr mult <valeur>", () => {
    expect(parseAtrSettingsCommand(["mult", "1.2"])).toEqual({ atrMultiplier: 1.2 });
  });

  test("atr period <entier>", () => {
    expect(parseAtrSettingsCommand(["period", "21"])).toEqual({ atrPeriod: 21 });
  });

  test("rejette une période non entière", () => {
    expect(parseAtrSettingsCommand(["period", "14.5"])).toBe(
      `période invalide : "14.5" — ${ATR_SETTINGS_USAGE}`,
    );
  });

  test("rejette une période sous le minimum (2)", () => {
    expect(parseAtrSettingsCommand(["period", "1"])).toBe(
      `période invalide : "1" — ${ATR_SETTINGS_USAGE}`,
    );
  });

  test("rejette rr/mult ≤ 0", () => {
    expect(parseAtrSettingsCommand(["rr", "0"])).toBe(`RR invalide : "0" — ${ATR_SETTINGS_USAGE}`);
    expect(parseAtrSettingsCommand(["mult", "-1"])).toBe(
      `multiplicateur invalide : "-1" — ${ATR_SETTINGS_USAGE}`,
    );
  });

  test("atr timeframe <M5|M15|H1>, insensible à la casse", () => {
    expect(parseAtrSettingsCommand(["timeframe", "m15"])).toEqual({ atrTimeframe: "M15" });
    expect(parseAtrSettingsCommand(["timeframe", "H1"])).toEqual({ atrTimeframe: "H1" });
  });

  test("rejette un timeframe hors de la liste fermée (pas une string libre)", () => {
    expect(parseAtrSettingsCommand(["timeframe", "M30"])).toBe(
      `timeframe invalide : "M30" — ${ATR_SETTINGS_USAGE}`,
    );
    expect(parseAtrSettingsCommand(["timeframe"])).toBe(
      `timeframe invalide : "" — ${ATR_SETTINGS_USAGE}`,
    );
  });

  test("signale une sous-commande inconnue", () => {
    expect(parseAtrSettingsCommand(["bogus", "1"])).toBe(
      `sous-commande inconnue : "bogus" — ${ATR_SETTINGS_USAGE}`,
    );
  });

  test("signale une valeur non numérique", () => {
    expect(parseAtrSettingsCommand(["rr", "abc"])).toBe(
      `valeur invalide : "abc" — ${ATR_SETTINGS_USAGE}`,
    );
  });
});

describe("formatTradeSummary", () => {
  test("formate un résumé lisible", () => {
    const trade: PreparedTrade = {
      orderType: "MARKET",
      tradeSide: "BUY",
      entryPrice: 4100.2,
      stopLoss: 4090,
      takeProfit: 4110,
      volume: 1000,
      volumeLots: 0.1,
      riskAmount: 100,
      riskPercent: 1,
      rewardAmount: 98,
    };
    expect(formatTradeSummary(trade)).toBe(
      "BUY MARKET 4100.20 · SL 4090.00 · TP 4110.00 · 0.10 lots · risque 100.00 (1%)",
    );
  });
});
