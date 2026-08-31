import type { StructureReading } from "../../structure/bias.ts";
import { computeScalpDirection, type ScalpBias } from "../../structure/confluence.ts";
import type { StructurePeriod } from "../../structure/fetch.ts";
import { STRUCTURE_PERIODS } from "../../structure/fetch.ts";
import type { StructureBias } from "../../structure/types.ts";
import { formatPriceOrDash } from "../format.ts";
import { DOWN, FLAT, UP } from "../glyphs.ts";
import { biasColor, theme } from "../theme.ts";

const LABELS: Record<StructurePeriod, string> = { M_1: "M1", M_5: "M5", M_15: "M15", H_1: "H1" };
const GLYPH: Record<StructureBias, string> = { bullish: UP, bearish: DOWN, neutral: FLAT };
const SCALP_GLYPH: Record<ScalpBias, string> = { bullish: UP, bearish: DOWN, mixed: FLAT };
const SCALP_LABEL: Record<ScalpBias, string> = {
  bullish: "BULLISH",
  bearish: "BEARISH",
  mixed: "NEUTRE",
};
/** `mixed` réutilise la même couleur atténuée que `neutral` (cf. theme.ts#biasColor) — vert/rouge
 * restent réservés à une direction franche. */
function scalpColor(bias: ScalpBias): string {
  if (bias === "bullish") return theme.green;
  if (bias === "bearish") return theme.red;
  return theme.textDim;
}

interface StructureBarProps {
  structure: Record<StructurePeriod, StructureReading> | undefined;
  errorMessage: string | undefined;
}

/** Ligne compacte, sans bordure de panel — purement informatif (aucune commande n'en dépend, cf.
 * structure/bias.ts), pas de raison de lui donner le même poids visuel qu'un panneau de données
 * exploitables (PositionsPanel, NewsPanel). Résistance = dernier swing haut confirmé, support =
 * dernier swing bas confirmé — définition simple, pas "le niveau juste au-dessus/en-dessous du prix
 * actuel" (cf. commentaire sur StructureReading). */
export function StructureBar({ structure, errorMessage }: StructureBarProps) {
  return (
    <box
      style={{
        flexDirection: "row",
        justifyContent: "center",
        alignItems: "center",
        columnGap: 2,
        paddingLeft: 2,
        paddingRight: 2,
        height: 1,
        flexShrink: 0,
        backgroundColor: theme.bg,
      }}
    >
      <text fg={theme.textMuted}>STRUCTURE</text>
      {structure === undefined ? (
        <text fg={theme.textDim}>{errorMessage ?? "chargement…"}</text>
      ) : (
        <>
          {STRUCTURE_PERIODS.map((period, index) => {
            const reading = structure[period];
            return (
              <box
                key={period}
                style={{ flexDirection: "row", alignItems: "center", columnGap: 1 }}
              >
                {index > 0 && <text fg={theme.textMuted}>·</text>}
                <text fg={theme.textDim}>{LABELS[period]}</text>
                <text fg={biasColor(reading.bias)}>{GLYPH[reading.bias]}</text>
                <text fg={theme.textMuted}>R</text>
                <text fg={theme.textDim}>{formatPriceOrDash(reading.resistance)}</text>
                <text fg={theme.textMuted}>S</text>
                <text fg={theme.textDim}>{formatPriceOrDash(reading.support)}</text>
              </box>
            );
          })}
          <ScalpDirectionBadge structure={structure} />
        </>
      )}
    </box>
  );
}

/** Verdict de scalp piloté par H1 avec confirmation M15/M5/M1 (cf. structure/confluence.ts) — seul
 * segment de la barre en `theme.text` (au lieu de `textDim`/`textMuted`) : c'est la seule
 * information de cette ligne pensée pour être lue d'un coup d'œil plutôt que consultée en détail,
 * le contraste de clarté fait le "pop" (cf. theme.ts, pas de couleur saturée hors vert/rouge). Le
 * glyphe est doublé en "strong" (M15, M5 ET M1 confirment tous les trois H1) pour le distinguer
 * visuellement du "moderate" (au moins un des trois neutre) sans introduire de troisième couleur. */
function ScalpDirectionBadge({
  structure,
}: {
  structure: Record<StructurePeriod, StructureReading>;
}) {
  const direction = computeScalpDirection(structure);
  const glyph = SCALP_GLYPH[direction.bias];
  return (
    <box style={{ flexDirection: "row", alignItems: "center", columnGap: 1 }}>
      <text fg={theme.textMuted}>·</text>
      <text fg={theme.textMuted}>SCALP</text>
      <text fg={scalpColor(direction.bias)}>
        {direction.strength === "strong" ? glyph + glyph : glyph}
      </text>
      <text fg={theme.text}>{SCALP_LABEL[direction.bias]}</text>
    </box>
  );
}
