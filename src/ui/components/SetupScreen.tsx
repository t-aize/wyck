import { useKeyboard } from "@opentui/react";
import { useState } from "react";
import type { AppConfig } from "../../config.ts";
import { writeConfig } from "../../config.ts";
import { CtraderClient } from "../../ctrader/client.ts";
import { validateFredApiKey } from "../../domain/macro.ts";
import { toMessage } from "../../errors.ts";
import { theme } from "../theme.ts";

const DEFAULT_URL = "https://mcp.ctrader.com/trading/mcp";
const FRED_KEY_URL = "https://fred.stlouisfed.org/docs/api/api_key.html";

interface SetupScreenProps {
  initial?: AppConfig;
  onConfigured: (config: AppConfig) => void;
  /** Fourni seulement pour un reconfig (commande `settings`) — pas de retour possible au tout premier lancement. */
  onCancel?: () => void;
}

type Step = "url" | "token" | "checkingToken" | "fred" | "checkingFred";

const INPUT_STEPS = new Set<Step>(["url", "token", "fred"]);

/**
 * Écran plein cadre (pas de popup à superposer : pas encore de client tant que la config
 * n'existe pas). Les trois champs (url, token, clé FRED) sont écrits en une seule fois, tous
 * ensemble, à la toute fin — jamais un sous-ensemble. Avant ce fichier, un reconfig via
 * `settings` ne redemandait que url/token et réécrivait la config sans la clé FRED, l'effaçant
 * silencieusement ; en la rendant obligatoire et en n'écrivant qu'un objet complet, ce chemin
 * "config partielle" n'existe plus.
 */
export function SetupScreen({ initial, onConfigured, onCancel }: SetupScreenProps) {
  const [step, setStep] = useState<Step>("url");
  const [url, setUrl] = useState(initial?.url ?? DEFAULT_URL);
  const [token, setToken] = useState(initial?.token ?? "");
  const [fredApiKey, setFredApiKey] = useState(initial?.fredApiKey ?? "");
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
    setStep("checkingToken");
    void (async () => {
      const client = new CtraderClient({ url, token: trimmed });
      try {
        await client.connect();
        await client.getBalance();
        setStep("fred");
      } catch (err) {
        setError(toMessage(err));
        setStep("token");
      } finally {
        void client.close();
      }
    })();
  }

  function submitFred(value: string) {
    const trimmed = value.trim();
    if (!trimmed) {
      setError("clé FRED requise — voir le lien ci-dessus pour en obtenir une gratuitement");
      return;
    }
    setFredApiKey(trimmed);
    setError(undefined);
    setStep("checkingFred");
    void (async () => {
      try {
        await validateFredApiKey(trimmed);
        const config: AppConfig = { url, token, fredApiKey: trimmed };
        writeConfig(config);
        onConfigured(config);
      } catch (err) {
        setError(toMessage(err));
        setStep("fred");
      }
    })();
  }

  const label =
    step === "url"
      ? "URL du serveur MCP (Entrée = valeur par défaut)"
      : step === "token"
        ? "Token MCP (cTrader Web → Settings → Remote MCP)"
        : step === "checkingToken"
          ? "Vérification de la connexion cTrader…"
          : step === "fred"
            ? `Clé API FRED — gratuite, sans carte : ${FRED_KEY_URL}`
            : "Vérification de la clé FRED…";

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
          width: 70,
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
        {INPUT_STEPS.has(step) && (
          <input
            key={step}
            focused
            value={step === "url" ? url : step === "token" ? token : fredApiKey}
            placeholder={step === "url" ? DEFAULT_URL : step === "token" ? "token…" : "clé FRED…"}
            onSubmit={(v) => {
              if (typeof v !== "string") return;
              if (step === "url") submitUrl(v);
              else if (step === "token") submitToken(v);
              else submitFred(v);
            }}
          />
        )}
        {error && <text fg={theme.red}>✗ {error}</text>}
        <text fg={theme.textMuted}>Entrée valider</text>
      </box>
    </box>
  );
}
