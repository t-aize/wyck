import { TextAttributes } from "@opentui/core";
import type { StructureSnapshot } from "../../domain/structure.ts";
import { alignLeft, alignRight, formatPrice } from "../format.ts";
import { DOWN, UP } from "../glyphs.ts";
import type { StructureRow } from "../hooks/useStructure.ts";
import { theme } from "../theme.ts";

interface StructurePanelProps {
  rows: StructureRow[] | undefined;
  errorMessage: string | undefined;
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

export function StructurePanel({ rows, errorMessage }: StructurePanelProps) {
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
        </>
      )}
    </box>
  );
}
