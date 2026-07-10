/**
 * Palette du panel : fond quasi noir façon terminal Bloomberg, croisé avec le
 * langage visuel des CLI d'agents (Claude Code / Codex / OpenCode) — panneaux
 * bordés avec titre inscrit dans la bordure, texte dense, peu de décoration.
 *
 * L'ambre est la seule couleur de marque : double sens assumé — c'est à la
 * fois la couleur historique de l'écran Bloomberg et celle de l'or (XAUUSD).
 * Vert/rouge restent strictement réservés à la direction et au P&L : c'est
 * une convention universelle chez les traders, pas un endroit pour innover.
 */
export const theme = {
  bg: "#0A0A0C",
  panelBg: "#111114",
  border: "#2A2A30",
  borderActive: "#F5A623",
  text: "#E8E6E1",
  textDim: "#8A8A90",
  textMuted: "#4A4A50",
  gold: "#F5A623",
  green: "#3DD68C",
  red: "#F0555A",
} as const;
