import { describe, expect, test } from "bun:test";
import {
  formatTradeSummary,
  MODIFY_USAGE,
  parseModifyCommand,
  parseRiskCommand,
  parseTradeCommand,
  RISK_USAGE,
  TRADE_USAGE,
} from "./commands.ts";
import type { PreparedTrade } from "./trading.ts";

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
