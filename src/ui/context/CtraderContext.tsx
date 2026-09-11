/**
 * Connexion cTrader (client, statut, catalogue de symboles, instrument actif).
 */

import { createContext, type ReactNode, useContext, useMemo, useState } from "react";
import { CtraderClient } from "../../ctrader/client/CtraderClient.ts";
import type { CtraderClientConfig } from "../../ctrader/client/CtraderClientConfig.ts";
import type { InstrumentSpecs } from "../../instrument/specs.ts";
import { useCtraderConnection } from "../hooks/useCtraderConnection.ts";

interface CtraderContextValue {
  client: CtraderClient;
  connected: boolean;
  catalog: InstrumentSpecs[];
  instrument: InstrumentSpecs | undefined;
  /** Raccourci : `instrument?.symbolId` — undefined tant que le catalogue n'est pas chargé. */
  symbolId: number | undefined;
  selectSymbol: (name: string) => boolean;
  connectionError: string | undefined;
  reportConnectionError: (message: string | undefined) => void;
}

const CtraderReactContext = createContext<CtraderContextValue | undefined>(undefined);

export function CtraderProvider({
  config,
  initialSymbol,
  children,
}: {
  config: CtraderClientConfig;
  initialSymbol: string;
  children: ReactNode;
}) {
  const [client] = useState(() => new CtraderClient(config));

  const { connected, catalog, instrument, connectionError, setConnectionError, selectSymbol } =
    useCtraderConnection(client, initialSymbol);

  const value: CtraderContextValue = useMemo(
    () => ({
      client,
      connected,
      catalog,
      instrument,
      symbolId: instrument?.symbolId,
      selectSymbol,
      connectionError,
      reportConnectionError: setConnectionError,
    }),
    [client, connected, catalog, instrument, selectSymbol, connectionError, setConnectionError],
  );

  return <CtraderReactContext.Provider value={value}>{children}</CtraderReactContext.Provider>;
}

export function useCtrader(): CtraderContextValue {
  const ctx = useContext(CtraderReactContext);
  if (!ctx) throw new Error("useCtrader() doit être utilisé sous <CtraderProvider>.");
  return ctx;
}
