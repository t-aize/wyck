import type { CalendarEvent } from "./schemas.ts";

/**
 * Biais XAUUSD anticipé à partir de forecast vs previous (la tendance attendue), PAS
 * actual vs forecast (la surprise à la publication) : le flux ForexFactory ne renseigne
 * jamais `actual`, même rétroactivement (vérifié par un fetch live) — limitation connue
 * du flux, pas nouvelle ici.
 */
export type GoldBias = "bullish" | "bearish" | "neutral";

/** "3.5%" → 3.5, "255K" → 255000, "-1.2M" → -1200000. */
function parseFigure(raw: string): number | undefined {
  const match = raw?.trim().match(/(-?[\d.,]+)\s*([KMB])?/i);
  if (!match) return undefined;
  const num = Number(match[1]!.replace(/,/g, ""));
  if (Number.isNaN(num)) return undefined;
  const multiplier = { K: 1e3, M: 1e6, B: 1e9 }[match[2]?.toUpperCase() as "K" | "M" | "B"] ?? 1;
  return num * multiplier;
}

interface PolarityRule {
  pattern: RegExp;
  polarity: "direct" | "inverse";
}

/**
 * Seuls les indicateurs avec un appel directionnel réel, étayé par la recherche (littérature
 * macro sur le driver rendement réel / fonction de réaction Fed). CPI/PCE/PPI/Trade Balance et
 * tous les events texte FOMC/Fed en sont délibérément absents (confirmé : pas de badge plutôt
 * qu'un pari) — ces derniers n'ont pas besoin d'exclusion explicite : ForexFactory ne renseigne
 * jamais forecast/previous pour eux (vérifié en live), ils retombent donc naturellement sur
 * `undefined`.
 */
const POLARITY_TABLE: PolarityRule[] = [
  // direct : lecture au-dessus du forecast = lecture Fed hawkish = baissier pour l'or
  { pattern: /non-?farm|\bnfp\b|\badp\b/i, polarity: "direct" }, // ADP inclus : même nature que NFP (privé vs officiel)
  { pattern: /\bgdp\b(?!.*price)/i, polarity: "direct" }, // exclut "GDP Price Index" (déflateur/inflation, ambigu comme CPI)
  { pattern: /\bpmi\b/i, polarity: "direct" },
  { pattern: /retail sales/i, polarity: "direct" },
  { pattern: /average hourly earnings/i, polarity: "direct" },
  { pattern: /\bjolts\b/i, polarity: "direct" },
  { pattern: /consumer (confidence|sentiment)/i, polarity: "direct" },
  { pattern: /building permits|housing starts|existing home sales/i, polarity: "direct" },
  // inverse : lecture au-dessus du forecast = marché du travail qui s'affaiblit = lecture Fed dovish = haussier pour l'or
  // "unemployment claims" : titre réel du flux ForexFactory pour les inscriptions hebdo au
  // chômage US (vérifié en live) — "jobless claims" n'apparaît jamais tel quel dans ce flux.
  { pattern: /unemployment (rate|claims)/i, polarity: "inverse" },
  { pattern: /jobless claims|claimant count/i, polarity: "inverse" },
];

export function goldBias(
  event: Pick<CalendarEvent, "title" | "forecast" | "previous">,
): GoldBias | undefined {
  const rule = POLARITY_TABLE.find((r) => r.pattern.test(event.title));
  if (!rule) return undefined;

  const forecast = parseFigure(event.forecast);
  const previous = parseFigure(event.previous);
  if (forecast === undefined || previous === undefined) return undefined;
  if (forecast === previous) return "neutral";

  const readingUp = forecast > previous;
  const goldUp = rule.polarity === "direct" ? !readingUp : readingUp;
  return goldUp ? "bullish" : "bearish";
}
