import { PRICE_SCALE } from "../constants.ts";
import { PARIS_TZ } from "../news/calendar/time.ts";

/** Prix cTrader : entier à l'échelle x10^5 (ex: 410177000 → 4101.77). */
export function formatPrice(raw: number | undefined, digits = 2): string {
  if (raw === undefined) return "—";
  return (raw / PRICE_SCALE).toFixed(digits);
}

/** Prix déjà affiché (pas à l'échelle x10^5) : simple fallback "—" si absent. */
export function formatPriceOrDash(price: number | undefined, digits = 2): string {
  return price === undefined ? "—" : price.toFixed(digits);
}

/** Montants cTrader (balance, P&L…) : entier à l'échelle x10^moneyDigits. */
export function formatMoney(raw: number | undefined, moneyDigits: number): string {
  if (raw === undefined) return "—";
  const value = raw / 10 ** moneyDigits;
  return value.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

export function formatClock(date: Date): string {
  return date.toLocaleTimeString("fr-FR", { hour12: false, timeZone: PARIS_TZ });
}

/** Durée compacte : "2j14h", "3h05", "12m". */
function formatDuration(ms: number): string {
  const clamped = Math.max(0, ms);
  const totalMinutes = Math.floor(clamped / 60_000);
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor((totalMinutes % 1440) / 60);
  const minutes = totalMinutes % 60;
  if (days > 0) return `${days}j${hours}h`;
  if (hours > 0) return `${hours}h${String(minutes).padStart(2, "0")}`;
  return `${minutes}m`;
}

/** Temps relatif : "dans 2h" / "il y a 12m". */
export function formatRelative(deltaMs: number): string {
  const label = formatDuration(Math.abs(deltaMs));
  return deltaMs >= 0 ? `dans ${label}` : `il y a ${label}`;
}

/** Aligne à gauche sur une largeur fixe (colonnes texte), tronque avec "…" si trop long. */
export function alignLeft(value: string, width: number): string {
  if (value.length > width)
    return width <= 1 ? value.slice(0, width) : `${value.slice(0, width - 1)}…`;
  return value.padEnd(width);
}

/** Aligne à droite sur une largeur fixe (colonnes numériques), tronque avec "…" si trop long. */
export function alignRight(value: string, width: number): string {
  if (value.length > width)
    return width <= 1 ? value.slice(0, width) : `…${value.slice(value.length - width + 1)}`;
  return value.padStart(width);
}

const SPARKLINE_BLOCKS = "▁▂▃▄▅▆▇█";

/**
 * Série de valeurs → sparkline Unicode (8 niveaux de blocs, min→max de la série passée). Une série
 * plate (min === max, y compris une valeur unique) rend le niveau médian partout plutôt que de
 * diviser par zéro — pas de "tendance" à montrer, mais pas de crash non plus.
 */
export function sparkline(values: number[]): string {
  if (values.length === 0) return "";
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min;
  return values
    .map((value) => {
      const normalized = range === 0 ? 0.5 : (value - min) / range;
      const level = Math.min(
        SPARKLINE_BLOCKS.length - 1,
        Math.floor(normalized * SPARKLINE_BLOCKS.length),
      );
      return SPARKLINE_BLOCKS[level];
    })
    .join("");
}
