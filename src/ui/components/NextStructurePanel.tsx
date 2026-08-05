import { TextAttributes } from "@opentui/core";
import type { PendingBreak } from "../../domain/smc/structure.ts";
import { alignLeft, formatPrice } from "../format.ts";
import { DOWN, UP } from "../glyphs.ts";
import type { StructureRow } from "../hooks/useStructure.ts";
import { theme } from "../theme.ts";

interface NextStructurePanelProps {
  rows: StructureRow[] | undefined;
  errorMessage: string | undefined;
}

const COLUMNS = { tf: 5, side: 17 } as const;

function headerRow() {
  return (
    <text fg={theme.textDim} attributes={TextAttributes.BOLD}>
      {alignLeft("TF", COLUMNS.tf)}
      {alignLeft(`INT ${UP}`, COLUMNS.side)}
      {alignLeft(`INT ${DOWN}`, COLUMNS.side)}
      {alignLeft(`SWING ${UP}`, COLUMNS.side)}
      {alignLeft(`SWING ${DOWN}`, COLUMNS.side)}
    </text>
  );
}

/** "BOS 2415.30" / "CHoCH 2398.10" en toutes lettres, "—" si rien à surveiller de ce côté (déjà
 * cassé, aucun nouveau pivot reformé depuis — cf. domain/smc/structure.ts). */
function pendingText(pending: PendingBreak | undefined): string {
  return pending ? `${pending.type} ${formatPrice(pending.level)}` : "—";
}

function pendingSpan(pending: PendingBreak | undefined, bullish: boolean) {
  const color = pending ? (bullish ? theme.green : theme.red) : theme.textDim;
  return <span fg={color}>{alignLeft(pendingText(pending), COLUMNS.side)}</span>;
}

function NextStructureRowLine({ row }: { row: StructureRow }) {
  return (
    <text>
      <span fg={theme.text}>{alignLeft(row.label, COLUMNS.tf)}</span>
      {pendingSpan(row.internal.nextBullish, true)}
      {pendingSpan(row.internal.nextBearish, false)}
      {pendingSpan(row.snapshot.nextBullish, true)}
      {pendingSpan(row.snapshot.nextBearish, false)}
    </text>
  );
}

/**
 * Prochain BOS/CHoCH par timeframe, à deux échelles (comme le script SMC de LuxAlgo) : "interne"
 * (fenêtre de pivot courte, `internalLength`) pour les retournements à petite échelle, "swing"
 * (fenêtre `length`, la même que le tableau SMC MTF STRUCTURE) pour la structure haut niveau. Pour
 * chaque échelle et chaque côté (haussier/baissier) : le niveau encore surveillé et ce que sa
 * cassure produirait — BOS si ça continue la tendance en cours à cette échelle, CHoCH si ça
 * l'inverse (cf. domain/smc/structure.ts). Panneau séparé du tableau structure : lecture "à
 * surveiller" à part de la lecture "état actuel".
 */
export function NextStructurePanel({ rows, errorMessage }: NextStructurePanelProps) {
  return (
    <box
      title=" PROCHAIN BOS / CHOCH "
      titleColor={theme.gold}
      style={{
        flexDirection: "column",
        flexShrink: 0,
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
            <NextStructureRowLine key={row.label} row={row} />
          ))}
        </>
      )}
    </box>
  );
}
