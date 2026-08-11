/**
 * Deuxième (et dernier, cf. docs/ARCHITECTURE.md §8) Context transversal de l'app : la connexion
 * cTrader (client, runtime Effect pour l'injection de dépendance de prepareTrade/prepareAtrTrade —
 * cf. §4.1 —, statut, symbolId). Absorbe la construction du client/runtime et l'ancien
 * `useCtraderConnection` — ce hook devient un détail d'implémentation interne à ce fichier, plus
 * importé ailleurs. `reportConnectionError` remplace le `setConnectionError` brut qu'exposait
 * `useCtraderConnection` (seul hook du projet à exposer un setter React direct plutôt qu'une
 * fonction d'action).
 */

import { Layer, ManagedRuntime } from "effect";
import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import type { AppConfig } from "../../config.ts";
import { CtraderClient, CtraderClientLive } from "../../ctrader/client.ts";
import { useCtraderConnection } from "../hooks/useCtraderConnection.ts";

interface CtraderContextValue {
  client: CtraderClientLive;
  /** Réservé à `prepareTrade`/`prepareAtrTrade` (injection Effect, cf. docs/ARCHITECTURE.md §4.1) —
   * tout le reste consomme `client` directement. */
  runtime: ManagedRuntime.ManagedRuntime<CtraderClient, never>;
  connected: boolean;
  symbolId: number | undefined;
  connectionError: string | undefined;
  reportConnectionError: (message: string | undefined) => void;
}

const CtraderReactContext = createContext<CtraderContextValue | undefined>(undefined);

export function CtraderProvider({ config, children }: { config: AppConfig; children: ReactNode }) {
  const [client] = useState(() => new CtraderClientLive(config));
  const runtime = useMemo(
    () => ManagedRuntime.make(Layer.succeed(CtraderClient, client)),
    [client],
  );
  useEffect(() => () => void runtime.dispose(), [runtime]);

  const { connected, symbolId, connectionError, setConnectionError } = useCtraderConnection(client);

  const value: CtraderContextValue = {
    client,
    runtime,
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
