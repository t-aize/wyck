import { useKeyboard } from "@opentui/react";
import { useState } from "react";
import type { AppConfig } from "../../config.ts";
import { writeConfig } from "../../config.ts";
import { CtraderClient } from "../../ctrader/client.ts";
import { toMessage } from "../../errors.ts";
import { theme } from "../theme.ts";

const DEFAULT_URL = "https://mcp.ctrader.com/trading/mcp";

interface SetupScreenProps {
  initial?: AppConfig;
  onConfigured: (config: AppConfig) => void;
  /** Fourni seulement pour un reconfig (commande `settings`) — pas de retour possible au tout premier lancement. */
  onCancel?: () => void;
}

type Step = "url" | "token" | "checking";

/** Écran plein cadre (pas de popup à superposer : pas encore de client tant que la config n'existe pas). */
export function SetupScreen({ initial, onConfigured, onCancel }: SetupScreenProps) {
  const [step, setStep] = useState<Step>("url");
  const [url, setUrl] = useState(initial?.url ?? DEFAULT_URL);
  const [token, setToken] = useState(initial?.token ?? "");
  const [error, setError] = useState<string>();

  useKeyboard((key) => {
    if (key.name === "escape" && onCancel) onCancel();
  });

  function submitUrl(value: string) {
    const trimmed = value.trim() || DEFAULT_URL;
    if (!URL.canParse(trimmed)) {
      setError(`URL invalide : "${trimmed}"`);
      return;
    }
    setUrl(trimmed);
    setError(undefined);
    setStep("token");
  }

  function submitToken(value: string) {
    const trimmed = value.trim();
    if (!trimmed) {
      setError("token requis");
      return;
    }
    setToken(trimmed);
    setError(undefined);
    setStep("checking");
    void (async () => {
      const client = new CtraderClient({ url, token: trimmed });
      try {
        await client.connect();
        await client.getBalance();
        writeConfig({ url, token: trimmed });
        onConfigured({ url, token: trimmed });
      } catch (err) {
        setError(toMessage(err));
        setStep("token");
      } finally {
        void client.close();
      }
    })();
  }

  const label =
    step === "url"
      ? "URL du serveur MCP (Entrée = valeur par défaut)"
      : step === "token"
        ? "Token MCP (cTrader Web → Settings → Remote MCP)"
        : "Vérification de la connexion…";

  return (
    <box
      style={{
        flexDirection: "column",
        width: "100%",
        height: "100%",
        backgroundColor: theme.bg,
        justifyContent: "center",
        alignItems: "center",
      }}
    >
      <box
        title=" CONFIGURATION MCP CTRADER "
        titleColor={theme.gold}
        style={{
          flexDirection: "column",
          width: 60,
          border: true,
          borderColor: theme.gold,
          backgroundColor: theme.panelBg,
          paddingLeft: 2,
          paddingRight: 2,
          paddingTop: 1,
          paddingBottom: 1,
          rowGap: 1,
        }}
      >
        <text fg={theme.textDim}>{label}</text>
        {step !== "checking" && (
          <input
            key={step}
            focused
            value={step === "url" ? url : token}
            placeholder={step === "url" ? DEFAULT_URL : "token…"}
            onSubmit={(v) =>
              typeof v === "string" ? (step === "url" ? submitUrl(v) : submitToken(v)) : undefined
            }
          />
        )}
        {error && <text fg={theme.red}>✗ {error}</text>}
        <text fg={theme.textMuted}>Entrée valider</text>
      </box>
    </box>
  );
}
