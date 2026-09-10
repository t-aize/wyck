/**
 * Tables de correspondance — le seul endroit où vivent les listes « magiques »
 * (tickers d'indices, bases crypto, devises ForexFactory).
 *
 * Les noms cTrader varient selon le broker (`US100` vs `USTEC` vs `NAS100`,
 * suffixes `.r` / `_SB`). On normalise le ticker puis on cherche ici.
 */

/** Devises que le calendrier ForexFactory tague vraiment (colonne `country`). */
export const FF_CURRENCIES = new Set([
  "USD",
  "EUR",
  "GBP",
  "JPY",
  "AUD",
  "CAD",
  "CHF",
  "NZD",
  "CNY",
  "HKD",
  "SGD",
  "MXN",
  "ZAR",
  "SEK",
  "NOK",
  "TRY",
  "PLN",
  "DKK",
  "CZK",
  "HUF",
  "INR",
  "KRW",
  "BRL",
]);

/**
 * Devises ISO pour classer une paire en `forex` (lot size), même si ForexFactory
 * ne les tague pas. Distinct de {@link FF_CURRENCIES} : on ne les écoute pas
 * dans le calendrier s'ils n'y sont pas.
 */
export const ISO_CURRENCIES = new Set([
  ...FF_CURRENCIES,
  "THB",
  "IDR",
  "MYR",
  "PHP",
  "ILS",
  "AED",
  "SAR",
  "TWD",
  "CLP",
  "COP",
  "ARS",
  "RON",
  "BGN",
  "ISK",
  "PKR",
  "EGP",
  "VND",
  "UAH",
  "PEN",
]);

/**
 * Suffixe broker collé au ticker. Pepperstone `.a`, IC `.r`, `_SB`, mini `m`
 * (`XAUUSDm`). Appliqué une fois : `EURUSD.r` → `EURUSD`.
 */
export const BROKER_SUFFIX = /(?:[._][A-Z0-9]+|m)$/i;

/**
 * Indice → devise « home » ForexFactory. Clés en ticker **normalisé**
 * (sans suffixe) ou en nom d'actif de base cTrader.
 */
export const INDEX_HOME: Record<string, string> = {
  US100: "USD",
  NAS100: "USD",
  USTEC: "USD",
  NDX: "USD",
  NDX100: "USD",
  NQ100: "USD",
  NASUSD: "USD",
  SPX: "USD",
  DJIA: "USD",
  US500: "USD",
  SPX500: "USD",
  SP500: "USD",
  US30: "USD",
  DJ30: "USD",
  DJI: "USD",
  US2000: "USD",
  RUSSELL: "USD",
  GER40: "EUR",
  GER30: "EUR",
  DE40: "EUR",
  DAX: "EUR",
  FRA40: "EUR",
  CAC: "EUR",
  CAC40: "EUR",
  EU50: "EUR",
  ESTX50: "EUR",
  STOXX50: "EUR",
  SPA35: "EUR",
  IBEX: "EUR",
  IT40: "EUR",
  NED25: "EUR",
  UK100: "GBP",
  FTSE: "GBP",
  FTSE100: "GBP",
  JPN225: "JPY",
  JP225: "JPY",
  NI225: "JPY",
  NIKKEI: "JPY",
  AUS200: "AUD",
  AU200: "AUD",
  ASX200: "AUD",
  HK50: "HKD",
  HK33: "HKD",
  HSI: "HKD",
  CHINA50: "CNY",
  CHI50: "CNY",
  SUI20: "CHF",
  SMI: "CHF",
};

/** Bases métaux (codes ISO 4217-like + alias cTrader GOLD/SILVER). */
export const METAL_BASES = new Set(["XAU", "XAG", "XPT", "XPD", "GOLD", "SILVER", "PALL", "PLAT"]);

/** Bases crypto les plus courantes chez les brokers cTrader. */
export const CRYPTO_BASES = new Set([
  "BTC",
  "XBT",
  "ETH",
  "LTC",
  "XRP",
  "SOL",
  "ADA",
  "BNB",
  "DOGE",
  "DOG",
  "DOT",
  "AVAX",
  "LINK",
  "UNI",
  "MATIC",
  "POL",
  "TRX",
  "XLM",
  "XMR",
  "ETC",
  "FIL",
  "AAVE",
  "TON",
  "SUI",
  "PEPE",
  "SHIB",
  "BCH",
  "ATOM",
  "NEAR",
  "APT",
  "ARB",
  "OP",
]);

/** Pétrole / gaz — tickers cTrader (`USOIL`) et codes (`XTI`, `XBR`). */
export const ENERGY_BASES = new Set([
  "XTI",
  "XBR",
  "XNG",
  "WTI",
  "BRENT",
  "USOIL",
  "UKOIL",
  "NATGAS",
  "NGAS",
  "OIL",
]);

/** Titres hors devise qui restent pertinents pour un métal. */
export const METAL_KEYWORDS = /gold|silver|precious metal|\bxau\b|\bxag\b|platinum|palladium/i;

/** Titres hors devise pertinents pour une crypto. */
export const CRYPTO_KEYWORDS = /bitcoin|\bbtc\b|ethereum|\beth\b|crypto|digital asset/i;

/** Titres hors devise pertinents pour l'énergie (inventaires EIA, etc.). */
export const ENERGY_KEYWORDS = /\boil\b|crude|brent|\bwti\b|gasoline|natural gas|inventor/i;

/** Titres hors devise pertinents pour un indice (rare, mais ça arrive). */
export const INDEX_KEYWORDS =
  /nasdaq|s&p|dow jones|\bdax\b|\bftse\b|nikkei|equity market|stock market/i;

/**
 * Regex qui ne matche jamais. Le forex n'a pas besoin de mots-clés de titre :
 * les deux devises du profil suffisent. On garde un `RegExp` (pas `undefined`)
 * pour que {@link NewsProfile.keywords} reste un seul type.
 */
export const NO_KEYWORDS = /(?!)/;

/** Quotes stables qui ne sont pas des codes FF (`USDT` → on raisonne en USD). */
export const QUOTE_ALIASES: Record<string, string> = { USDT: "USD", USDC: "USD", CNH: "CNY" };

/**
 * Candidats de suffixe quote, **les plus longs d'abord** pour ne pas couper
 * `BTCUSDT` en `BTCUS` + `DT` : `USDT` doit gagner contre `USD`.
 */
export const QUOTE_CANDIDATES = [...ISO_CURRENCIES, "USDT", "USDC"].sort(
  (a, b) => b.length - a.length,
);
