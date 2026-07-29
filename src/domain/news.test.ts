import { describe, expect, test } from "bun:test";
import {
  type CalendarEvent,
  classifyImpact,
  goldDirection,
  isGoldRelevant,
  type NewsImpact,
} from "./news.ts";

function event(overrides: Partial<CalendarEvent>): CalendarEvent {
  return {
    title: "Some Indicator",
    country: "USD",
    date: "2026-07-29T12:30:00Z",
    impact: "High",
    forecast: "",
    previous: "",
    timestamp: 0,
    ...overrides,
  };
}

describe("classifyImpact", () => {
  test.each<[string, NewsImpact]>([
    ["High", "high"],
    [" medium ", "medium"],
    ["LOW", "low"],
    ["Holiday", "other"],
    ["", "other"],
  ])("classe %p en %p", (raw, expected) => {
    expect(classifyImpact(raw)).toBe(expected);
  });
});

describe("isGoldRelevant", () => {
  test("toute publication USD est pertinente", () => {
    expect(isGoldRelevant({ country: "USD", title: "Retail Sales m/m" })).toBe(true);
  });

  test("un titre mentionnant l'or est pertinent même hors USD", () => {
    expect(isGoldRelevant({ country: "EUR", title: "Central bank XAU reserves" })).toBe(true);
    expect(isGoldRelevant({ country: "CNY", title: "Gold import quota" })).toBe(true);
  });

  test("ni USD ni or ⇒ non pertinent", () => {
    expect(isGoldRelevant({ country: "EUR", title: "German ZEW Survey" })).toBe(false);
  });
});

describe("goldDirection", () => {
  test("indicateur pro-USD classique, prévision > précédent ⇒ baissier pour l'or", () => {
    const e = event({ title: "Non-Farm Payrolls", forecast: "200K", previous: "180K" });
    expect(goldDirection(e)).toBe("down");
  });

  test("indicateur pro-USD classique, prévision < précédent ⇒ haussier pour l'or", () => {
    const e = event({ title: "Retail Sales m/m", forecast: "0.2%", previous: "0.5%" });
    expect(goldDirection(e)).toBe("up");
  });

  test("indicateur inversé (chômage), prévision > précédent ⇒ haussier pour l'or", () => {
    const e = event({ title: "Unemployment Claims", forecast: "230K", previous: "210K" });
    expect(goldDirection(e)).toBe("up");
  });

  test("prévision === précédent ⇒ flat", () => {
    const e = event({ title: "CPI m/m", forecast: "0.3%", previous: "0.3%" });
    expect(goldDirection(e)).toBe("flat");
  });

  test("chiffres non parsables ⇒ undefined", () => {
    const e = event({ title: "FOMC Statement", forecast: "", previous: "" });
    expect(goldDirection(e)).toBeUndefined();
  });

  test("gère les suffixes K/M/B", () => {
    const e = event({ title: "Trade Balance", forecast: "1.2M", previous: "800K" });
    expect(goldDirection(e)).toBe("down");
  });
});
