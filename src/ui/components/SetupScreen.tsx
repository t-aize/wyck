import { BunFileSystem } from "@effect/platform-bun";
import { useKeyboard } from "@opentui/react";
import { Effect } from "effect";
import { useState } from "react";
import type { AppConfig } from "../../config.ts";
import { AppConfigSchema, writeConfig } from "../../config.ts";
import { CtraderClientLive } from "../../ctrader/client.ts";
import { toMessage } from "../../errors.ts";
import { theme } from "../theme.ts";

const DEFAULT_URL = "https://mcp.ctrader.com/trading/mcp";

interface SetupScreenProps {
  initial?: AppConfig;
  onConfigured: (config: AppConfig) => void;
  /** Fourni seulement pour un reconfig (commande `settings`) — pas de retour possible au tout premier lancement. */
  onCancel?: () => void;
}

type Step = "url" | "token" | "checkingToken";

const INPUT_STEPS = new Set<Step>(["url", "token"]);

/**
 * Écran plein cadre (pas de popup à superposer : pas encore de client tant que la config
 * n'existe pas). Les deux champs (url, token) sont écrits en une seule fois, tous ensemble, à la
 * toute fin — jamais un sous-ensemble.
 */
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
    if (!AppConfigSchema.shape.url.safeParse(trimmed).success) {
      setError(`URL invalide : "${trimmed}"`);
      return;
    }
    setUrl(trimmed);
    setError(undefined);
    setStep("token");
  }

  function submitToken(value: string) {
    const trimmed = value.trim();
    if (!AppConfigSchema.shape.token.safeParse(trimmed).success) {
      setError("token requis");
      return;
    }
    setToken(trimmed);
    setError(undefined);
    setStep("checkingToken");
    void (async () => {
      const client = new CtraderClientLive({ url, token: trimmed });
      try {
        await client.connect();
        await Effect.runPromise(client.getBalance());
        const config: AppConfig = { url, token: trimmed };
        await Effect.runPromise(Effect.provide(writeConfig(config), BunFileSystem.layer));
        onConfigured(config);
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
        : "Vérification de la connexion cTrader…";

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
        titleColor={theme.accent}
        style={{
          flexDirection: "column",
          width: 70,
          border: true,
          borderColor: theme.accent,
          backgroundColor: theme.panelBg,
          paddingLeft: 2,
          paddingRight: 2,
          paddingTop: 1,
          paddingBottom: 1,
          rowGap: 1,
        }}
      >
        <text fg={theme.textDim}>{label}</text>
        {INPUT_STEPS.has(step) && (
          <input
            key={step}
            focused
            value={step === "url" ? url : token}
            placeholder={step === "url" ? DEFAULT_URL : "token…"}
            onSubmit={(v) => {
              if (typeof v !== "string") return;
              if (step === "url") submitUrl(v);
              else submitToken(v);
            }}
          />
        )}
        {error && <text fg={theme.red}>✗ {error}</text>}
        <text fg={theme.textMuted}>Entrée valider</text>
      </box>
    </box>
  );
}
