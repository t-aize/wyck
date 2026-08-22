import { describe, expect, test } from "bun:test";
import { parseFiniteNumber, parseFlags, parseOptionalPrice, parsePrice } from "./_shared.ts";

describe("parseFiniteNumber", () => {
  test("parses a valid number string", () => {
    expect(parseFiniteNumber("42")).toBe(42);
    expect(parseFiniteNumber("-3.5")).toBe(-3.5);
  });

  test("returns undefined for empty, absent, or non-numeric input", () => {
    expect(parseFiniteNumber(undefined)).toBeUndefined();
    expect(parseFiniteNumber("")).toBeUndefined();
    expect(parseFiniteNumber("   ")).toBeUndefined();
    expect(parseFiniteNumber("abc")).toBeUndefined();
    expect(parseFiniteNumber("Infinity")).toBeUndefined();
  });
});

describe("parsePrice", () => {
  test("rounds to the API's 2-decimal precision", () => {
    expect(parsePrice("2000.005")).toBe(2000.01);
    expect(parsePrice("2000.001")).toBe(2000);
  });

  test("rejects zero and negative prices", () => {
    expect(parsePrice("0")).toBeUndefined();
    expect(parsePrice("-5")).toBeUndefined();
  });

  test("rejects non-numeric input", () => {
    expect(parsePrice("abc")).toBeUndefined();
    expect(parsePrice(undefined)).toBeUndefined();
  });
});

describe("parseOptionalPrice", () => {
  test("absent input returns an empty object, no error", () => {
    expect(parseOptionalPrice(undefined, "sl")).toEqual({});
  });

  test("valid input returns { value }", () => {
    expect(parseOptionalPrice("1990", "sl")).toEqual({ value: 1990 });
  });

  test("invalid input returns a labeled error", () => {
    expect(parseOptionalPrice("abc", "sl")).toEqual({ error: 'sl invalide : "abc"' });
    expect(parseOptionalPrice("-5", "tp")).toEqual({ error: 'tp invalide : "-5"' });
  });
});

describe("parseFlags", () => {
  const aliases = { sl: ["--sl", "-sl"], tp: ["--tp", "-tp"] };

  test("parses known flags in any order, aliases included", () => {
    expect(parseFlags(["--sl", "1990", "-tp", "2020"], aliases)).toEqual({
      sl: "1990",
      tp: "2020",
    });
  });

  test("rejects an unknown flag", () => {
    expect(parseFlags(["--risk", "1"], aliases)).toBe('option inconnue : "--risk"');
  });

  test("rejects a missing value at the end of the args", () => {
    expect(parseFlags(["--sl"], aliases)).toBe("valeur manquante pour --sl");
  });

  test("rejects a value that is itself a known flag, instead of silently absorbing it", () => {
    expect(parseFlags(["--sl", "--tp", "10"], aliases)).toBe("valeur manquante pour --sl");
  });
});
