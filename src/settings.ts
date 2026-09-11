/**
 * Réglages utilisateur (URL + token MCP cTrader, plus les préférences applicatives comme
 * `atrRefreshEnabled`), persistés dans le homedir plutôt que dans `.env` : un `.exe` compilé
 * (`bun build --compile`) n'embarque pas `.env` et peut être lancé depuis n'importe quel dossier
 * (double-clic), donc `process.cwd()` au runtime n'a aucune raison de contenir le fichier — les
 * réglages doivent vivre à un endroit stable indépendant du dossier de lancement. dev et .exe
 * lisent donc la même chose ici. Seule la commande `settings` (cf. commands/settings.ts) écrit ici
 * — pas d'assistant de configuration séparé, cf. App.tsx#EMPTY_APP_CONFIG. I/O disque déléguée à
 * `storage/jsonFile.ts` (cf. ce fichier pour la discipline lecture/écriture partagée).
 */

import { createCipheriv, createDecipheriv, randomBytes, scryptSync } from "node:crypto";
import { hostname, userInfo } from "node:os";
import { join } from "node:path";
import type { FileSystem } from "@effect/platform";
import type { PlatformError } from "@effect/platform/Error";
import { Effect } from "effect";
import { z } from "zod";
import { APP_DATA_DIR, DEFAULT_SYMBOL } from "./constants.ts";
import type { CtraderClientConfig } from "./ctrader/client/CtraderClientConfig.ts";
import { TrendbarPeriod } from "./ctrader/protocol/TrendbarPeriod.ts";
import { readJsonFile, writeJsonFile } from "./storage/jsonFile.ts";
import { ATR_PERIOD, ATR_TIMEFRAME } from "./trading/atr.ts";

const SETTINGS_PATH = join(APP_DATA_DIR, "settings.json");

/** Les trois champs sont optionnels : `url`/`token` peuvent être réglés l'un sans l'autre (commandes
 * `settings url`/`settings token` séparées, cf. commands/settings.ts) — un fichier qui n'a encore
 * que l'un des deux est un état normal, pas une anomalie. Avant l'introduction de ce schéma, `token`
 * était supposé toujours présent (`as {url: string; token: string; ...}` non vérifié) : régler
 * seulement `url` produisait un fichier sans `token`, et `decrypt(undefined)` levait une exception
 * silencieusement avalée par `readConfig` — qui retournait alors `undefined`, faisant réapparaître
 * l'app comme totalement non configurée (`EMPTY_APP_CONFIG`) alors que `url` était bel et bien
 * sauvegardée sur disque. Correction : chaque champ est maintenant lu indépendamment. */
const SettingsFileSchema = z.object({
  url: z.string().optional(),
  token: z.string().optional(),
  atrRefreshEnabled: z.boolean().optional(),
  atrPeriod: z.number().int().positive().optional(),
  atrTimeframe: z.nativeEnum(TrendbarPeriod).optional(),
  symbol: z.string().min(1).optional(),
});
type SettingsFile = z.infer<typeof SettingsFileSchema>;

// ponytail: clé dérivée de la machine/l'utilisateur (pas de dépendance keychain
// cross-platform genre keytar). Ça évite le token en clair dans le fichier — protège
// contre une lecture accidentelle (cat, backup cloud, capture d'écran) — mais pas
// contre un attaquant qui a déjà un accès complet à la session de cet utilisateur (il
// peut recalculer la même clé). Passer à DPAPI/keychain natif si ce niveau ne suffit pas.
function deriveKey(): Buffer {
  return scryptSync(`${hostname()}:${userInfo().username}:aurum-config`, "aurum-config-salt", 32);
}

// GCM standard : IV 96 bits (12 octets), tag d'authentification 128 bits (16 octets) — la taille
// réellement utilisée n'a pas changé, seulement rendue explicite pour Node (DEP0182 : depuis peu,
// createDecipheriv sans authTagLength émet un warning au premier setAuthTag()).
const AUTH_TAG_LENGTH = 16;

function encrypt(text: string): string {
  const iv = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", deriveKey(), iv, { authTagLength: AUTH_TAG_LENGTH });
  const ciphertext = Buffer.concat([cipher.update(text, "utf8"), cipher.final()]);
  return Buffer.concat([iv, cipher.getAuthTag(), ciphertext]).toString("base64");
}

function decrypt(payload: string): string {
  const raw = Buffer.from(payload, "base64");
  const iv = raw.subarray(0, 12);
  const authTag = raw.subarray(12, 12 + AUTH_TAG_LENGTH);
  const ciphertext = raw.subarray(12 + AUTH_TAG_LENGTH);
  const decipher = createDecipheriv("aes-256-gcm", deriveKey(), iv, {
    authTagLength: AUTH_TAG_LENGTH,
  });
  decipher.setAuthTag(authTag);
  return Buffer.concat([decipher.update(ciphertext), decipher.final()]).toString("utf8");
}

/** Config + réglages persistés : `CtraderClientConfig` (url/token, cf. ctrader/client.ts) plus les
 * réglages applicatifs qui s'accumulent dans le même fichier (aujourd'hui : juste
 * `atrRefreshEnabled`, cf. useAtrAutoRefresh.ts). Un seul type plutôt que deux fichiers séparés :
 * même contrainte de fusion (§writeConfig), pas la peine de dupliquer toute la mécanique
 * lecture/écriture pour une poignée de booléens. */
export interface AppConfig extends CtraderClientConfig {
  /** Rafraîchissement auto du SL/TP des ordres ATR en attente (toutes les 60s, cf.
   * useAtrAutoRefresh.ts). Absent du fichier = activé (comportement par défaut). */
  atrRefreshEnabled: boolean;
  /** Période et timeframe de l'ATR utilisé en mode trade ATR (cf. trading/atr.ts#fetchAtr) et par le
   * refresh auto (useAtrAutoRefresh.ts). Réglables via `settings atrperiod`/`settings atrtimeframe`
   * (commands/settings.ts). Absents du fichier = défauts historiques (ATR_PERIOD/ATR_TIMEFRAME). */
  atrPeriod: number;
  atrTimeframe: TrendbarPeriod;
  /** Symbole actif (nom cTrader, ex. XAUUSD / US100). Absent du fichier = DEFAULT_SYMBOL. */
  symbol: string;
}

/** Réglages "vides" utilisés par App.tsx tant qu'aucun fichier n'existe encore (premier lancement)
 * ou tant qu'url/token n'ont pas été réglés — l'app se rend quand même normalement dans cet état
 * (cf. CtraderClient#isConfigured), pas d'assistant de configuration séparé à afficher. */
export const EMPTY_APP_CONFIG: AppConfig = {
  url: "",
  token: "",
  atrRefreshEnabled: true,
  atrPeriod: ATR_PERIOD,
  atrTimeframe: ATR_TIMEFRAME,
  symbol: DEFAULT_SYMBOL,
};

/**
 * `undefined` seulement si le fichier est totalement absent, illisible, ou d'une forme rejetée par
 * `SettingsFileSchema` — redemande implicitement les réglages dans ce cas plutôt que planter (cf.
 * App.tsx#EMPTY_APP_CONFIG). Un `token` présent mais déchiffrable seulement sur une autre machine
 * (ciphertext corrompu, clé dérivée différente) invalide aussi tout le résultat, pour la même
 * raison : rien d'exploitable à en tirer. En revanche `url`/`token` simplement absents du fichier
 * (réglés indépendamment, cf. `SettingsFileSchema`) retombent sur `""`, pas sur `undefined` global
 * — c'est le bug corrigé par ce schéma (cf. son commentaire). Dépend de `FileSystem`
 * (`@effect/platform`) plutôt que d'appeler `node:fs` en dur : un test peut fournir une
 * implémentation en mémoire sans jamais toucher `~/.aurum/settings.json`. Consommateur :
 * `fsRuntime.runPromise(readConfig())` (cf. src/utils/effectRuntime.ts) — l'I/O de
 * `@effect/platform-bun` est réellement async (contrairement à l'ancien `node:fs` synchrone), donc
 * `Effect.runSync` n'est pas utilisable ici (`AsyncFiberException` à l'exécution, vérifié en
 * pratique) : App.tsx charge les réglages dans un `useEffect`, pas dans l'initializer de
 * `useState`. Le module `Config` d'Effect cible des variables d'environnement, pas un fichier JSON
 * chiffré sur disque — pas le bon outil ici malgré le nom.
 */
export function readConfig(): Effect.Effect<AppConfig | undefined, never, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const file = yield* readJsonFile(SETTINGS_PATH, SettingsFileSchema);
    if (file === undefined) return undefined;

    // `decrypt` peut throw (ciphertext corrompu, ou déchiffrable seulement sur une autre machine) —
    // Effect.try (pas Effect.sync) pour ne pas laisser l'exception s'échapper en defect non catché.
    // Pas de decrypt du tout si `token` n'est pas encore réglé : `""` n'est pas un ciphertext.
    const token = yield* Effect.try(() => (file.token ? decrypt(file.token) : "")).pipe(
      Effect.orElseSucceed(() => undefined),
    );
    if (token === undefined) return undefined;

    return {
      url: file.url ?? "",
      token,
      atrRefreshEnabled: file.atrRefreshEnabled ?? true,
      atrPeriod: file.atrPeriod ?? ATR_PERIOD,
      atrTimeframe: file.atrTimeframe ?? ATR_TIMEFRAME,
      symbol: file.symbol ?? DEFAULT_SYMBOL,
    };
  });
}

/** Échoue tel quel (disque plein, permissions) — contrairement à `readConfig`, un échec
 * d'écriture doit remonter à l'utilisateur (cf. commands/settings.ts), pas être avalé.
 *
 * `patch` plutôt qu'un `AppConfig` complet : fusionne uniquement les clés fournies par-dessus le
 * fichier existant, au lieu de le réécrire en entier. Avant ce fix, un appel qui ne voulait changer
 * que `atrRefreshEnabled` (cf. useCommandRouter.ts) aurait effacé `url`/`token` — et vice-versa, un
 * changement d'url/token via `settings url`/`settings token` aurait effacé les réglages. Le contenu
 * existant est lu via `SettingsFileSchema` (pas via `readConfig`, qui déchiffre `token` — inutile
 * ici, on ne fait que le recopier tel quel si `patch.token` n'est pas fourni). */
export function writeConfig(
  patch: Partial<AppConfig>,
): Effect.Effect<void, PlatformError, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const existing = (yield* readJsonFile(SETTINGS_PATH, SettingsFileSchema)) ?? {};

    const merged: SettingsFile = {
      ...existing,
      ...(patch.url !== undefined ? { url: patch.url } : {}),
      ...(patch.token !== undefined ? { token: encrypt(patch.token) } : {}),
      ...(patch.atrRefreshEnabled !== undefined
        ? { atrRefreshEnabled: patch.atrRefreshEnabled }
        : {}),
      ...(patch.atrPeriod !== undefined ? { atrPeriod: patch.atrPeriod } : {}),
      ...(patch.atrTimeframe !== undefined ? { atrTimeframe: patch.atrTimeframe } : {}),
      ...(patch.symbol !== undefined ? { symbol: patch.symbol } : {}),
    };

    yield* writeJsonFile(APP_DATA_DIR, SETTINGS_PATH, merged, { mode: 0o600 });
  });
}
