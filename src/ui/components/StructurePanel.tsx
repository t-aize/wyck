import { TextAttributes } from "@opentui/core";
import type { Conviction, OverallBias, SecondaryBias } from "../../domain/smc/bias.ts";
import type { StructureSnapshot } from "../../domain/smc/structure.ts";
import { alignLeft, alignRight, formatPrice } from "../format.ts";
import { DOWN, UP } from "../glyphs.ts";
import type { StructureRow } from "../hooks/useStructure.ts";
import { theme } from "../theme.ts";

interface StructurePanelProps {
  rows: StructureRow[] | undefined;
  errorMessage: string | undefined;
  bias: OverallBias;
}

const COLUMNS = { tf: 4, sweep: 2, bias: 9, struct: 8, high: 8, low: 8 } as const;

function biasLabel(bias: StructureSnapshot["bias"]): string {
  return bias === 1 ? "HAUSSIER" : bias === -1 ? "BAISSIER" : "NEUTRE";
}

function biasColor(bias: StructureSnapshot["bias"]): string {
  return bias === 1 ? theme.green : bias === -1 ? theme.red : theme.textDim;
}

function structLabel(snapshot: StructureSnapshot): string {
  if (!snapshot.signalType) return "—";
  return `${snapshot.signalType} ${snapshot.signalDir === 1 ? UP : DOWN}`;
}

function structColor(snapshot: StructureSnapshot): string {
  return snapshot.signalDir === 1
    ? theme.green
    : snapshot.signalDir === -1
      ? theme.red
      : theme.textDim;
}

function headerRow() {
  return (
    <text fg={theme.textDim} attributes={TextAttributes.BOLD}>
      {alignLeft("TF", COLUMNS.tf)}
      {alignLeft(" ", COLUMNS.sweep)}
      {alignLeft("BIAIS", COLUMNS.bias)}
      {alignLeft("STRUCT", COLUMNS.struct)}
      {alignRight("HIGH", COLUMNS.high)}
      {alignRight("LOW", COLUMNS.low)}
    </text>
  );
}

/** Flèche colorée quand le dernier swing a été balayé (mèche au-delà, clôture repassée à l'intérieur). */
function StructureRowLine({ row }: { row: StructureRow }) {
  const { snapshot } = row;
  const sweepGlyph = snapshot.sweepLow ? UP : snapshot.sweepHigh ? DOWN : " ";
  const sweepColor = snapshot.sweepLow
    ? theme.green
    : snapshot.sweepHigh
      ? theme.red
      : theme.textDim;

  return (
    <text>
      <span fg={theme.text}>{alignLeft(row.label, COLUMNS.tf)}</span>
      <span fg={sweepColor}>{alignLeft(sweepGlyph, COLUMNS.sweep)}</span>
      <span fg={biasColor(snapshot.bias)}>{alignLeft(biasLabel(snapshot.bias), COLUMNS.bias)}</span>
      <span fg={structColor(snapshot)}>{alignLeft(structLabel(snapshot), COLUMNS.struct)}</span>
      <span fg={theme.textDim}>{alignRight(formatPrice(snapshot.swingHigh), COLUMNS.high)}</span>
      <span fg={theme.textDim}>{alignRight(formatPrice(snapshot.swingLow), COLUMNS.low)}</span>
    </text>
  );
}

function convictionLabel(conviction: Conviction): string {
  return conviction === "strong" ? "forte" : conviction === "moderate" ? "modérée" : "faible";
}

/** Repli 1H affiché seulement quand D1 ET 4H sont tous les deux neutres (cf. domain/smc/bias.ts) — une lecture plus courte pour qui veut trader quand même sans biais clair sur les plus hauts TF. */
function SecondaryBiasLine({ secondary }: { secondary: SecondaryBias }) {
  return (
    <text>
      <span fg={theme.textMuted}>↳ repli 1H (D1/4H neutres) : </span>
      <span fg={biasColor(secondary.direction)}>{biasLabel(secondary.direction)}</span>
      <span fg={theme.textMuted}>
        {`  ·  conviction ${convictionLabel(secondary.conviction)}  —  ${secondary.confirmationCount}/2 TF confirment`}
      </span>
    </text>
  );
}

/** Synthèse d'une phrase : ancrage D1+4H, confirmation/macro en support, calendrier en avertissement — jamais l'inverse (cf. domain/smc/bias.ts). */
function BiasLine({ bias }: { bias: OverallBias }) {
  const caution = bias.caution && (
    <span
      fg={theme.gold}
    >{`  ⚠ ${bias.caution.event.title} dans ${bias.caution.minutesUntil}m`}</span>
  );

  if (bias.direction === 0) {
    const anchorText = bias.anchor
      ? ` (D1 ${biasLabel(bias.anchor.d1)} / 4H ${biasLabel(bias.anchor.h4)})`
      : "";
    // Conflit réel (D1/4H directionnels et opposés) mis en avant en rouge — distinct du cas banal
    // (un des deux encore neutre) qui reste discret, cf. le commentaire sur anchorConflict.
    const primary = bias.anchorConflict ? (
      <text>
        <span fg={theme.red} attributes={TextAttributes.BOLD}>
          {`⚠ BIAIS GLOBAL : D1/4H EN CONFLIT${anchorText}`}
        </span>
        {caution}
      </text>
    ) : (
      <text>
        <span fg={theme.textDim}>{`BIAIS GLOBAL : PAS DE BIAIS CLAIR${anchorText}`}</span>
        {caution}
      </text>
    );
    return (
      <>
        {primary}
        {bias.secondary && <SecondaryBiasLine secondary={bias.secondary} />}
      </>
    );
  }

  return (
    <text>
      <span fg={theme.textDim}>BIAIS GLOBAL : </span>
      <span fg={biasColor(bias.direction)} attributes={TextAttributes.BOLD}>
        {biasLabel(bias.direction)}
      </span>
      {/* conviction toujours définie ici : garantie par computeOverallBias dès que direction !== 0 */}
      <span fg={theme.textDim}>
        {`  ·  conviction ${convictionLabel(bias.conviction!)}  —  ${bias.confirmationCount}/3 TF confirment, ${bias.macroAlignedCount}/3 facteurs macro alignés`}
      </span>
      {caution}
    </text>
  );
}

export function StructurePanel({ rows, errorMessage, bias }: StructurePanelProps) {
  return (
    <box
      title=" SMC MTF STRUCTURE "
      titleColor={theme.gold}
      style={{
        flexDirection: "column",
        flexGrow: 1,
        flexBasis: 0,
        border: true,
        borderColor: theme.border,
        backgroundColor: theme.bg,
        paddingLeft: 1,
        paddingRight: 1,
      }}
    >
      {errorMessage ? (
        <text fg={theme.red}>{errorMessage}</text>
      ) : rows === undefined ? (
        <text fg={theme.textDim}>chargement…</text>
      ) : (
        <>
          {headerRow()}
          {rows.map((row) => (
            <StructureRowLine key={row.label} row={row} />
          ))}
          <BiasLine bias={bias} />
        </>
      )}
    </box>
  );
}
