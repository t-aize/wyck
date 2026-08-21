import { Effect } from "effect";
import { useState } from "react";
import type { GetPositionsResult } from "../../ctrader/schemas.ts";
import {
  CANCEL_USAGE,
  MODIFY_USAGE,
  parseModifyCommand,
  parseRiskCommand,
  parseTradeCommand,
  RISK_USAGE,
  resolveCancelTargets,
  TRADE_USAGE,
} from "../../domain/commands.ts";
import { prepareTrade } from "../../domain/trading.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";
import type { CancelConfirm } from "./useCancelConfirm.ts";
import type { ModifyConfirm } from "./useModifyConfirm.ts";
import type { TradeConfirm } from "./useTradeConfirm.ts";

const COMMAND_LIST = "trade  modify  cancel  risk  settings  refresh  clear  help";

/** Détail affiché par `help <commande>` — réutilise les mêmes chaînes d'usage que les erreurs de
 * parsing. */
const COMMAND_HELP: Record<string, string> = {
  trade: TRADE_USAGE,
  modify: MODIFY_USAGE,
  cancel: CANCEL_USAGE,
  risk: RISK_USAGE,
  settings: "settings — reconfigure l'URL/le token MCP",
  refresh: "refresh — force une actualisation immédiate du marché et du calendrier",
  clear: "clear — efface le message de feedback",
  help: "help [commande] — liste les commandes, ou détaille l'usage d'une commande précise",
};

export interface CommandRouter {
  runCommand: (raw: string) => void;
}

/** Parsing + dispatch des commandes du CommandBar — un des 4 hooks issus de l'éclatement de
 * useOrderActions.ts. Sur succès de `trade`/`modify`/`cancel`, délègue au hook de confirmation
 * correspondant plutôt que de posséder lui-même cet état. */
export function useCommandRouter(opts: {
  positions: GetPositionsResult | undefined;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  onReconfigure: () => void;
  tradeConfirm: Pick<TradeConfirm, "proposeTrade">;
  modifyConfirm: Pick<ModifyConfirm, "proposeModify">;
  cancelConfirm: Pick<CancelConfirm, "proposeCancel">;
}): CommandRouter {
  const {
    positions,
    refreshMarket,
    refreshNews,
    onReconfigure,
    tradeConfirm,
    modifyConfirm,
    cancelConfirm,
  } = opts;
  const { client, symbolId } = useCtrader();
  const { setFeedback } = useFeedback();

  // Réglé via la commande `risk`, jamais persisté : repart à zéro à chaque lancement plutôt que de
  // continuer à trader silencieusement sur un risque défini une session précédente et oublié.
  const [defaultRiskPercent, setDefaultRiskPercent] = useState<number>();

  function runCommand(raw: string) {
    const trimmed = raw.trim();
    if (!trimmed) return;
    const [commandRaw, ...args] = trimmed.split(/\s+/);
    const command = commandRaw?.toLowerCase() ?? "";

    switch (command) {
      case "help": {
        const target = args[0]?.toLowerCase();
        if (!target) {
          setFeedback({ kind: "info", message: `commandes : ${COMMAND_LIST}` });
          return;
        }
        const detail = COMMAND_HELP[target];
        setFeedback(
          detail
            ? { kind: "info", message: detail }
            : { kind: "error", message: `commande inconnue : ${target} — ${COMMAND_LIST}` },
        );
        return;
      }
      case "settings":
        onReconfigure();
        return;
      case "refresh":
        setFeedback({ kind: "info", message: "actualisation…" });
        void Promise.all([refreshMarket(), refreshNews({ force: true })]).then(() => {
          setFeedback({ kind: "success", message: "actualisé" });
        });
        return;
      case "clear":
        setFeedback({ kind: "info", message: "" });
        return;
      case "risk": {
        if (args.length === 0) {
          setFeedback({
            kind: "info",
            message:
              defaultRiskPercent === undefined
                ? `aucun risque par défaut — ${RISK_USAGE}`
                : `risque par défaut : ${defaultRiskPercent}%`,
          });
          return;
        }
        const parsedRisk = parseRiskCommand(args);
        if (typeof parsedRisk === "string") {
          setFeedback({ kind: "error", message: parsedRisk });
          return;
        }
        setDefaultRiskPercent(parsedRisk);
        setFeedback({
          kind: "success",
          message: `risque par défaut réglé à ${parsedRisk}% pour cette session`,
        });
        return;
      }
      case "trade": {
        if (!symbolId) {
          setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
          return;
        }
        const onFailed = (error: unknown) =>
          setFeedback({ kind: "error", message: toMessage(error) });

        const parsed = parseTradeCommand(args, defaultRiskPercent);
        if (typeof parsed === "string") {
          setFeedback({ kind: "error", message: parsed });
          return;
        }
        setFeedback({ kind: "info", message: "calcul en cours…" });
        void Effect.runPromise(prepareTrade(client, symbolId, parsed)).then(
          tradeConfirm.proposeTrade,
          onFailed,
        );
        return;
      }
      case "modify": {
        const parsed = parseModifyCommand(args);
        if (typeof parsed === "string") {
          setFeedback({ kind: "error", message: parsed });
          return;
        }
        // `modify` ne cible que les ordres en attente pour l'instant, pas les positions ouvertes —
        // cf. `CtraderPositionSchema` dans ctrader/schemas.ts pour la forme désormais confirmée.
        const order = positions?.orders.find((o) => o.orderId === parsed.id);
        if (!order) {
          setFeedback({
            kind: "error",
            message: `ordre en attente ${parsed.id} introuvable`,
          });
          return;
        }
        modifyConfirm.proposeModify(order, parsed.stopLoss, parsed.takeProfit);
        return;
      }
      case "cancel": {
        const resolved = resolveCancelTargets(args, positions?.orders ?? []);
        if (resolved.kind === "rejected") {
          setFeedback({ kind: resolved.level, message: resolved.message });
          return;
        }
        cancelConfirm.proposeCancel(resolved.orders);
        return;
      }
      default:
        setFeedback({
          kind: "error",
          message: `commande inconnue : ${command} — help pour la liste`,
        });
    }
  }

  return { runCommand };
}
