import type { CtraderPosition } from "@aurum/ctrader";
import { TextAttributes } from "@opentui/core";
import type { InstrumentSpecs } from "../../../instrument/specs.ts";
import { toLots } from "../../../utils/priceMath.ts";
import { alignLeft, alignRight, formatPriceOrDash } from "../../format.ts";
import { DOWN, UP } from "../../glyphs.ts";
import type { SpotQuote } from "../../hooks/useMarketData.ts";
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

function PositionRow({
  p,
  specs,
  quote,
  active,
}: {
  p: CtraderPosition;
  specs: InstrumentSpecs | undefined;
  quote: SpotQuote | undefined;
  active: boolean;
}) {
  const sideLabel = p.side === "BUY" ? `${UP} BUY` : p.side === "SELL" ? `${DOWN} SELL` : "—";
  const mid = quote === undefined ? undefined : (quote.bidPrice + quote.askPrice) / 2;
  const nearest = nearestPipsLabel(mid, p.stopLoss, p.takeProfit, specs?.pipSize);
  const digits = specs?.digits ?? 2;
  const lots =
    p.volume === undefined || specs === undefined ? undefined : toLots(p.volume, specs.lotSize);
  const name = specs?.symbolName ?? (p.symbolId !== undefined ? `#${p.symbolId}` : "—");
  const symbolFg = active ? theme.accent : theme.text;

  return (
    <text>
      <span fg={symbolFg} attributes={active ? TextAttributes.BOLD : TextAttributes.NONE}>
        {alignLeft(name, COLUMNS.symbol)}
      </span>
      <span fg={sideColor(p.side)}>{alignLeft(sideLabel, COLUMNS.side)}</span>
      <span fg={theme.text}>{alignRight(lots?.toFixed(2) ?? "—", COLUMNS.volume)}</span>
      <span fg={theme.text}>{alignRight(formatPriceOrDash(p.entry, digits), COLUMNS.entry)}</span>
      <span fg={theme.red}>{alignRight(formatPriceOrDash(p.stopLoss, digits), COLUMNS.sl)}</span>
      <span fg={theme.green}>
        {alignRight(formatPriceOrDash(p.takeProfit, digits), COLUMNS.tp)}
      </span>
      <span fg={theme.textDim}>{alignRight(nearest, COLUMNS.dist)}</span>
    </text>
  );
}

interface PositionsTableProps {
  positions: CtraderPosition[];
  catalogById: ReadonlyMap<number, InstrumentSpecs>;
  quotesBySymbolId: ReadonlyMap<number, SpotQuote>;
  activeSymbolId: number | undefined;
}

export function PositionsTable({
  positions,
  catalogById,
  quotesBySymbolId,
  activeSymbolId,
}: PositionsTableProps) {
  return (
    <box flexDirection="column">
      <text fg={theme.textMuted} attributes={TextAttributes.BOLD}>
        — positions ouvertes (tous symboles) —
      </text>
      <scrollbox
        scrollX
        scrollY={false}
        wrapperOptions={{ flexGrow: 0, flexShrink: 0 }}
        viewportOptions={{ flexGrow: 0, flexShrink: 0 }}
        contentOptions={{ minHeight: 0 }}
        style={{ flexDirection: "column" }}
      >
        {headerRow()}
        {positions.length === 0 ? (
          <text fg={theme.textMuted}>aucune position ouverte</text>
        ) : (
          positions.map((p, index) => (
            <PositionRow
              key={p.id ?? index}
              p={p}
              specs={p.symbolId !== undefined ? catalogById.get(p.symbolId) : undefined}
              quote={p.symbolId !== undefined ? quotesBySymbolId.get(p.symbolId) : undefined}
              active={activeSymbolId !== undefined && p.symbolId === activeSymbolId}
            />
          ))
        )}
      </scrollbox>
    </box>
  );
}
