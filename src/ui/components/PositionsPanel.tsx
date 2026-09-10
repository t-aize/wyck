import type { GetPositionsResult } from "../../ctrader/schemas.ts";
import type { InstrumentSpecs } from "../../instrument/specs.ts";
import type { SpotQuote } from "../hooks/useMarketData.ts";
import { theme } from "../theme.ts";
import { isUnmapped } from "./positions/columns.ts";
import { OrdersTable } from "./positions/OrdersTable.tsx";
import { PositionsTable } from "./positions/PositionsTable.tsx";

interface PositionsPanelProps {
  positions: GetPositionsResult | undefined;
  catalog: InstrumentSpecs[];
  quotesBySymbolId: ReadonlyMap<number, SpotQuote>;
  atrOrderIds: Set<number>;
  activeSymbolId: number | undefined;
}

export function PositionsPanel({
  positions,
  catalog,
  quotesBySymbolId,
  atrOrderIds,
  activeSymbolId,
}: PositionsPanelProps) {
  const openPositions = positions?.positions ?? [];
  const pendingOrders = positions?.orders ?? [];
  const catalogById = new Map(catalog.map((item) => [item.symbolId, item]));
  const hasUnmapped = openPositions.some(isUnmapped);

  return (
    <box
      title=" POSITIONS "
      titleColor={theme.accent}
      flexDirection="column"
      flexShrink={0}
      border
      borderColor={theme.border}
      backgroundColor={theme.bg}
      paddingLeft={1}
      paddingRight={1}
    >
      {hasUnmapped && (
        <text fg={theme.red}>
          ⚠ format de position inattendu — certaines valeurs peuvent être fausses ou manquantes
        </text>
      )}
      {positions === undefined ? (
        <text fg={theme.textDim}>chargement…</text>
      ) : (
        <>
          <PositionsTable
            positions={openPositions}
            catalogById={catalogById}
            quotesBySymbolId={quotesBySymbolId}
            activeSymbolId={activeSymbolId}
          />
          <OrdersTable
            orders={pendingOrders}
            catalogById={catalogById}
            quotesBySymbolId={quotesBySymbolId}
            atrOrderIds={atrOrderIds}
            activeSymbolId={activeSymbolId}
          />
        </>
      )}
    </box>
  );
}
