/**
 * Palette du panel : fond quasi noir façon terminal Bloomberg, croisé avec le
 * langage visuel des CLI d'agents (Claude Code / Codex / OpenCode) — panneaux
 * bordés avec titre inscrit dans la bordure, texte dense, peu de décoration.
 *
 * Monochrome façon shadcn/ui (thème "neutral", dark mode) : leur `--foreground`
 * et leur `--primary` sont tous les deux du gris pur (chroma OKLCH = 0), jamais
 * une teinte saturée — le "pop" vient du contraste de clarté, pas de la couleur.
 * `accent` reprend ce principe (gris clair, ni blanc pur ni saturé) à la place
 * de l'ambre d'origine. Vert/rouge restent strictement réservés à la direction
 * et au P&L : convention universelle chez les traders, pas un endroit pour innover.
 */
import type { TradeSide } from "@aurum/ctrader";
import type { KillzoneId, MarketSessionId } from "../sessions/types.ts";
import type { StructureBias } from "../structure/types.ts";

export const theme = {
  bg: "#0A0A0A",
  panelBg: "#141414",
  border: "#27272A",
  borderActive: "#E5E5E5",
  text: "#FAFAFA",
  textDim: "#A3A3A3",
  textMuted: "#525252",
  accent: "#E5E5E5",
  green: "#3DD68C",
  red: "#F0555A",
  /** Badge de bascule mode ATR dans CommandBar (Shift+Tab) — teinte dédiée, distincte des badges
   * de session/killzone ci-dessous et de vert/rouge (direction/P&L). */
  atrMode: "#FB923C",
  /** Une couleur par session de marché (cf. sessions/catalog.ts) — même teinte que la killzone de
   * la même place (Londres/New York) pour rester cohérent visuellement entre les deux badges. */
  sessions: {
    sydney: "#F472B6",
    tokyo: "#FBBF24",
    london: "#60A5FA",
    newYork: "#A78BFA",
  } as const satisfies Record<MarketSessionId, string>,
  /** Une couleur par killzone ICT (cf. sessions/catalog.ts) — distinctes de vert/rouge, réservés
   * à la direction et au P&L. Asia (20h-00h NY) reprend la teinte de la session Tokyo, dont elle
   * recouvre les horaires. */
  killzones: {
    asia: "#FBBF24", // = sessions.tokyo, cf. commentaire ci-dessus
    london: "#60A5FA",
    newYork: "#A78BFA",
    londonClose: "#38BDF8",
  } as const satisfies Record<KillzoneId, string>,
} as const;

/** BUY = vert, SELL = rouge, absent (mapping incomplet, cf. CtraderPositionSchema) = atténué —
 * même convention partout où un side/tradeSide colore une ligne (modales de confirmation,
 * PositionsPanel). */
export function sideColor(side: TradeSide | undefined): string {
  if (side === "BUY") return theme.green;
  if (side === "SELL") return theme.red;
  return theme.textDim;
}

/** P&L latent : vert si positif ou nul, rouge si négatif, atténué si indisponible. */
export function pnlColor(pnl: number | undefined): string {
  if (pnl === undefined) return theme.textDim;
  return pnl >= 0 ? theme.green : theme.red;
}

/** Biais de structure (cf. structure/bias.ts) : bullish = vert, bearish = rouge, neutral/indisponible
 * = atténué — même convention que sideColor()/pnlColor() ci-dessus. */
export function biasColor(bias: StructureBias | undefined): string {
  if (bias === "bullish") return theme.green;
  if (bias === "bearish") return theme.red;
  return theme.textDim;
}
