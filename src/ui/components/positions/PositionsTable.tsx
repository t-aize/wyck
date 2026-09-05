import { TextAttributes } from "@opentui/core";
import { SYMBOL } from "../../../constants.ts";
import type { CtraderPosition } from "../../../ctrader/schemas.ts";
import { alignLeft, alignRight, formatPriceOrDash } from "../../format.ts";
import { DOWN, UP } from "../../glyphs.ts";
import { sideColor, theme } from "../../theme.ts";
import { COLUMNS, nearestPipsLabel } from "./columns.ts";

function headerRow() {
  return (
    <text fg={theme.textDim} attributes={TextAttributes.BOLD}>
      {alignLeft("SYMBOL", COLUMNS.symbol)}
      {alignLeft("SIDE", COLUMNS.side)}
      {alignRight("VOL", COLUMNS.volume)}
      {alignRight("ENTRY", COLUMNS.entry)}
      {alignRight("SL", COLUMNS.sl)}
      {alignRight("TP", COLUMNS.tp)}
      {alignRight("PROCHE", COLUMNS.dist)}
    </text>
  );
}

function PositionRow({ p, mid }: { p: CtraderPosition; mid: number | undefined }) {
  const sideLabel = p.side === "BUY" ? `${UP} BUY` : p.side === "SELL" ? `${DOWN} SELL` : "—";
  const nearest = nearestPipsLabel(mid, p.stopLoss, p.takeProfit);

  return (
    <text>
      <span fg={theme.text}>
        {alignLeft(p.id === undefined ? SYMBOL : String(p.id), COLUMNS.symbol)}
      </span>
      <span fg={sideColor(p.side)}>{alignLeft(sideLabel, COLUMNS.side)}</span>
      <span fg={theme.text}>{alignRight(p.volumeLots?.toFixed(2) ?? "—", COLUMNS.volume)}</span>
      <span fg={theme.text}>{alignRight(formatPriceOrDash(p.entry), COLUMNS.entry)}</span>
      <span fg={theme.red}>{alignRight(formatPriceOrDash(p.stopLoss), COLUMNS.sl)}</span>
      <span fg={theme.green}>{alignRight(formatPriceOrDash(p.takeProfit), COLUMNS.tp)}</span>
      <span fg={theme.textDim}>{alignRight(nearest, COLUMNS.dist)}</span>
    </text>
  );
}

interface PositionsTableProps {
  positions: CtraderPosition[];
  mid: number | undefined;
}

/** Pas de colonnes SWAP/P&L : ces deux champs se sont avérés peu fiables en pratique (souvent
 * indisponibles selon le compte/broker) — même jeu de colonnes que OrdersTable.tsx (symbole,
 * side, volume, prix, SL, TP, distance) plutôt que d'afficher des "—" sans grande valeur. */
export function PositionsTable({ positions, mid }: PositionsTableProps) {
  return (
    <box flexDirection="column">
      <text fg={theme.textMuted} attributes={TextAttributes.BOLD}>
        — positions ouvertes —
      </text>
      {/* Les colonnes sont à largeur fixe — sur un terminal plus étroit, mieux vaut pouvoir défiler
       * horizontalement que perdre des colonnes recadrées par le terminal. `scrollX` seul (pas de
       * flexGrow/height imposé) laisse la hauteur continuer à s'ajuster au contenu comme avant. */}
      <scrollbox
        scrollX
        scrollY={false}
        wrapperOptions={{ flexGrow: 0, flexShrink: 0 }}
        viewportOptions={{ flexGrow: 0, flexShrink: 0 }}
        contentOptions={{ minHeight: 0 }}
        style={{ flexDirection: "column" }}
      >
        {headerRow()}
        {positions.map((p, index) => (
          // positionId a une confiance haute (cf. CtraderPositionSchema) ; l'index reste un filet
          // pour le cas — signalé par le parent (PositionsPanel.tsx) — où le mapping échoue en pratique.
          <PositionRow key={p.id ?? index} p={p} mid={mid} />
        ))}
      </scrollbox>
    </box>
  );
}
