/**
 * `@aurum/news` — calendrier économique et lecture macro d'un symbole.
 *
 * Trois domaines, volontairement séparés :
 *
 * 1. **calendar** — aller chercher le flux ForexFactory (semaine en cours), le
 *    valider, le cacher un jour (fuseau Paris). Aucune idée de ce qu'est un
 *    XAUUSD ou un US100.
 * 2. **profile** — à partir d'un ticker cTrader (et idéalement de ses
 *    base/quote `get_assets`), produire un {@link NewsProfile} : classe d'actif,
 *    devises ForexFactory pertinentes, mots-clés de titre.
 * 3. **analysis** — pour un event + un profil : est-ce visible ? quel biais ?
 *
 * L'app n'importe que cette façade. Les chemins internes (`calendar/fetch.ts`,
 * etc.) ne font pas partie de l'API.
 *
 * @packageDocumentation
 */

export { type InstrumentBias, instrumentBias } from "./analysis/bias.ts";
export { parseFigure } from "./analysis/figures.ts";
export {
  type IndicatorKind,
  indicatorKind,
  type MacroImpulse,
  macroImpulse,
} from "./analysis/polarity.ts";
export {
  classifyImpact,
  isDefaultVisible,
  isHighImpact,
  isRelevant,
} from "./analysis/relevance.ts";
export { FetchCalendarError, fetchCalendar } from "./calendar/fetch.ts";
export type { CacheFile, CalendarEvent, NewsImpact } from "./calendar/schemas.ts";
export {
  CacheFileSchema,
  CalendarEventSchema,
  DecoratedCalendarEventSchema,
  NewsImpactSchema,
} from "./calendar/schemas.ts";
export { PARIS_TZ, parisDayKeyFormat } from "./calendar/time.ts";
export { classifyAssetClass } from "./profile/classify.ts";
export { newsProfile } from "./profile/profile.ts";
export { inferBaseQuote, normalizeSymbolName } from "./profile/symbol.ts";
export type { AssetClass, NewsProfile } from "./profile/types.ts";
