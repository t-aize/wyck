import type { CtraderSymbol } from "./CtraderSymbol.ts";

/** Enveloppe de `get_symbols`. */
export interface GetSymbolsResult {
  symbols: CtraderSymbol[];
}
