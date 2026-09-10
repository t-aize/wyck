import { describe, expect, test } from "bun:test";
import { digitsFor, lotSizeFor, pipSizeFor, volumeStep } from "./specs.ts";

describe("instrument specs heuristics", () => {
  test("forex EURUSD: 100k lot, 5 digits, pip 0.0001", () => {
    expect(lotSizeFor("forex", "EUR")).toBe(100_000);
    expect(digitsFor("forex", "EUR", "USD")).toBe(5);
    expect(pipSizeFor("forex", "EUR", "USD")).toBe(0.0001);
    expect(volumeStep(100_000)).toBe(100_000);
  });

  test("forex USDJPY: 3 digits, pip 0.01", () => {
    expect(digitsFor("forex", "USD", "JPY")).toBe(3);
    expect(pipSizeFor("forex", "USD", "JPY")).toBe(0.01);
  });

  test("gold: 100 oz lot, 2 digits, pip 0.1, 0.01 lot = 100 volume", () => {
    expect(lotSizeFor("metal", "XAU")).toBe(100);
    expect(digitsFor("metal", "XAU", "USD")).toBe(2);
    expect(pipSizeFor("metal", "XAU", "USD")).toBe(0.1);
    expect(volumeStep(100)).toBe(100);
  });

  test("index/crypto: lotSize 1", () => {
    expect(lotSizeFor("index", "US100")).toBe(1);
    expect(lotSizeFor("crypto", "BTC")).toBe(1);
    expect(volumeStep(1)).toBe(1);
  });
});
