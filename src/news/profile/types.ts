/**
 * Types du profil « news » d'un symbole — ce dont l'analyse a besoin, sans
 * rien savoir de cTrader (pas de `symbolId`, pas de lotSize).
 */

/**
 * Classe d'actif, du point de vue **macro** (quelles news, quel biais), pas
 * du point de vue courtage (lotSize vit dans `src/instrument/` côté app).
 *
 * - `forex` — deux devises ISO (EURUSD).
 * - `metal` — XAU/XAG/… coté dans une devise.
 * - `index` — US100, GER40, UK100… (ticker ≠ paire).
 * - `crypto` — BTC, ETH…
 * - `energy` — WTI, Brent, gaz.
 * - `other` — fallback : on se rabat sur la devise de cotation.
 */
export type AssetClass = "forex" | "metal" | "index" | "crypto" | "energy" | "other";

/**
 * Vue « pour filtrer et biaiser le calendrier.
 *
 * Produit par {@link newsProfile}. `countries` = codes devise ForexFactory
 * (`USD`, `EUR`…) : un event dont `country` est dans cette liste est pertinent.
 * `keywords` rattrape les titres hors devise (réserves d'or, Bitcoin ETF…).
 */
export interface NewsProfile {
  /** Nom tel que cTrader l'expose (suffixe broker conservé pour l'affichage). */
  symbolName: string;
  assetClass: AssetClass;
  /** Actif de base canonique (`EUR`, `XAU`, `US100`, `BTC`). */
  base: string;
  /** Devise de cotation canonique (`USD`, `EUR`… ; USDT → USD). */
  quote: string;
  /** Codes devise ForexFactory qui rendent un event pertinent. */
  countries: readonly string[];
  /** Mots-clés de titre propres à la classe ; `(?!)` pour le forex (inutile). */
  keywords: RegExp;
}
