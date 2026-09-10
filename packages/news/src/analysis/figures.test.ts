import { describe, expect, test } from "bun:test";
import { parseFigure } from "./figures.ts";

describe("parseFigure", () => {
  test("parses K/M/B/T suffixes and percents", () => {
    expect(parseFigure("255K")).toBe(255_000);
    expect(parseFigure("-1.2M")).toBe(-1_200_000);
    expect(parseFigure("768B")).toBe(768_000_000_000);
    expect(parseFigure("2.48T")).toBe(2.48e12);
    expect(parseFigure("0.3%")).toBe(0.3);
    expect(parseFigure("  11.8K ")).toBe(11_800);
  });

  test("strips thousands separators", () => {
    expect(parseFigure("1,234.5")).toBe(1234.5);
  });

  test("rejects empty, junk, and dual auction figures", () => {
    expect(parseFigure("")).toBeUndefined();
    expect(parseFigure("n/a")).toBeUndefined();
    expect(parseFigure("3.26|1.1")).toBeUndefined();
  });
});
