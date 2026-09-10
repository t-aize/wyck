/**
 * Biais **instrument** à partir d'un event + d'un {@link NewsProfile}.
 *
 * Pipeline :
 * 1. {@link indicatorKind} — famille de l'indicateur (ou stop : pas de pari).
 * 2. {@link parseFigure} forecast/previous (ou stop si absents).
 * 3. {@link macroImpulse} — hawkish / dovish **pour la devise**.
 * 4. Mapping devise → instrument, qui dépend de `profile.assetClass`.
 *
 * Mapping par classe :
 *
 * | Classe | Règle |
 * |--------|--------|
 * | forex BASE/QUOTE | hawkish base → haussier ; hawkish quote → baissier |
 * | métal | inverse de la devise de cotation (USD hawkish → or baissier) |
 * | indice / crypto / énergie | croissance hawkish → haussier (risk-on) ; inflation / taux hawkish → baissier ; chômage en hausse → baissier |
 *
 * `undefined` = on n'affiche pas de badge, on ne devine pas.
 */

import type { CalendarEvent } from "../calendar/schemas.ts";
import type { NewsProfile } from "../profile/types.ts";
import { parseFigure } from "./figures.ts";
import { indicatorKind, macroImpulse } from "./polarity.ts";

/** Direction anticipée pour l'instrument. `neutral` = forecast = previous. */
export type InstrumentBias = "bullish" | "bearish" | "neutral";

/**
 * Biais anticipé (forecast vs previous), ou `undefined` si on n'a pas d'appel
 * directionnel (indicateur hors table, figures absentes, devise hors profil).
 *
 * @example
 * // XAUUSD + NFP au-dessus du previous → Fed hawkish → or baissier
 * instrumentBias({ title: "Non-Farm Employment Change", country: "USD", forecast: "255K", previous: "230K" }, gold)
 * // → "bearish"
 *
 * // US100 + le même NFP → risk-on → indice haussier
 * instrumentBias({ …même event… }, us100)
 * // → "bullish"
 */
export function instrumentBias(
  event: Pick<CalendarEvent, "title" | "country" | "forecast" | "previous">,
  profile: NewsProfile,
): InstrumentBias | undefined {
  const kind = indicatorKind(event.title);
  if (!kind) return undefined;

  const forecast = parseFigure(event.forecast);
  const previous = parseFigure(event.previous);
  if (forecast === undefined || previous === undefined) return undefined;

  const impulse = macroImpulse(kind, forecast, previous);
  if (impulse === "neutral") return "neutral";

  const hawkish = impulse === "hawkish";
  const country = event.country;

  switch (profile.assetClass) {
    case "forex": {
      if (country === profile.base) return hawkish ? "bullish" : "bearish";
      if (country === profile.quote) return hawkish ? "bearish" : "bullish";
      return undefined;
    }
    case "metal": {
      // Inverse de la devise de cotation (typiquement USD).
      if (country === profile.quote || profile.countries.includes(country)) {
        return hawkish ? "bearish" : "bullish";
      }
      return undefined;
    }
    case "index":
    case "crypto":
    case "energy":
    case "other": {
      if (!profile.countries.includes(country)) return undefined;
      if (kind === "growth") return hawkish ? "bullish" : "bearish";
      if (kind === "inflation" || kind === "rates") return hawkish ? "bearish" : "bullish";
      // labor_slack : chômage en hausse → impulse dovish → baissier pour un risk-asset.
      return hawkish ? "bullish" : "bearish";
    }
  }
}
