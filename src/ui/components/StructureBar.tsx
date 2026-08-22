import type { StructureReading } from "../../structure/bias.ts";
import type { StructurePeriod } from "../../structure/fetch.ts";
import { STRUCTURE_PERIODS } from "../../structure/fetch.ts";
import type { StructureBias } from "../../structure/types.ts";
import { formatPriceOrDash } from "../format.ts";
import { DOWN, FLAT, UP } from "../glyphs.ts";
import { biasColor, theme } from "../theme.ts";

const LABELS: Record<StructurePeriod, string> = { M_5: "M5", M_15: "M15", H_1: "H1" };
const GLYPH: Record<StructureBias, string> = { bullish: UP, bearish: DOWN, neutral: FLAT };

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
        STRUCTURE_PERIODS.map((period, index) => {
          const reading = structure[period];
          return (
            <box key={period} style={{ flexDirection: "row", alignItems: "center", columnGap: 1 }}>
              {index > 0 && <text fg={theme.textMuted}>·</text>}
              <text fg={theme.textDim}>{LABELS[period]}</text>
              <text fg={biasColor(reading.bias)}>{GLYPH[reading.bias]}</text>
              <text fg={theme.textMuted}>R</text>
              <text fg={theme.textDim}>{formatPriceOrDash(reading.resistance)}</text>
              <text fg={theme.textMuted}>S</text>
              <text fg={theme.textDim}>{formatPriceOrDash(reading.support)}</text>
            </box>
          );
        })
      )}
    </box>
  );
}
