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
  /** Une couleur par session de marché (cf. domain/sessions.ts) — même teinte que la killzone de
   * la même place (Londres/New York) pour rester cohérent visuellement entre les deux badges. */
  sessions: {
    sydney: "#F472B6",
    tokyo: "#FBBF24",
    london: "#60A5FA",
    newYork: "#A78BFA",
  },
  /** Une couleur par killzone ICT (cf. domain/sessions.ts) — distinctes de vert/rouge, réservés
   * à la direction et au P&L. */
  killzones: {
    asia: "#FBBF24",
    london: "#60A5FA",
    newYork: "#A78BFA",
    londonClose: "#38BDF8",
  },
} as const;
