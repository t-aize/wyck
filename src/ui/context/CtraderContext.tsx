/**
 * Deuxième (et dernier) Context transversal de l'app : la connexion
 * cTrader (client, statut, symbolId). Absorbe la construction du client et l'ancien
 * `useCtraderConnection` — ce hook devient un détail d'implémentation interne à ce fichier, plus
 * importé ailleurs. `reportConnectionError` remplace le `setConnectionError` brut qu'exposait
 * `useCtraderConnection` (seul hook du projet à exposer un setter React direct plutôt qu'une
 * fonction d'action).
 */

import { createContext, type ReactNode, useContext, useState } from "react";
import { CtraderClient, type CtraderClientConfig } from "../../ctrader/client.ts";
import { useCtraderConnection } from "../hooks/useCtraderConnection.ts";

interface CtraderContextValue {
  client: CtraderClient;
  connected: boolean;
  symbolId: number | undefined;
  connectionError: string | undefined;
  reportConnectionError: (message: string | undefined) => void;
}

const CtraderReactContext = createContext<CtraderContextValue | undefined>(undefined);

export function CtraderProvider({
  config,
  children,
}: {
  config: CtraderClientConfig;
  children: ReactNode;
}) {
  const [client] = useState(() => new CtraderClient(config));

  const { connected, symbolId, connectionError, setConnectionError } = useCtraderConnection(client);

  const value: CtraderContextValue = {
    client,
    connected,
    symbolId,
    connectionError,
    reportConnectionError: setConnectionError,
  };

  return <CtraderReactContext.Provider value={value}>{children}</CtraderReactContext.Provider>;
}

export function useCtrader(): CtraderContextValue {
  const ctx = useContext(CtraderReactContext);
  if (!ctx) throw new Error("useCtrader() doit être utilisé sous <CtraderProvider>.");
  return ctx;
}
