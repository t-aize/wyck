/**
 * Config utilisateur (URL + token MCP cTrader), persistée dans le homedir plutôt que
 * dans `.env` : un `.exe` compilé (`bun build --compile`) n'embarque pas `.env` et peut
 * être lancé depuis n'importe quel dossier (double-clic), donc `process.cwd()` au
 * runtime n'a aucune raison de contenir le fichier — la config doit vivre à un endroit
 * stable indépendant du dossier de lancement. dev et .exe lisent donc la même chose ici.
 */

import { createCipheriv, createDecipheriv, randomBytes, scryptSync } from "node:crypto";
import { hostname, userInfo } from "node:os";
import { join } from "node:path";
import { FileSystem } from "@effect/platform";
import type { PlatformError } from "@effect/platform/Error";
import { Effect } from "effect";
import { APP_DATA_DIR } from "./constants.ts";
import type { CtraderClientConfig } from "./ctrader/client.ts";

const CONFIG_PATH = join(APP_DATA_DIR, "config.json");

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
}

/**
 * `undefined` si absent, JSON invalide, ou déchiffrable seulement sur une autre machine —
 * redemande la config dans ces cas plutôt que planter. Aucune validation de forme au-delà de ça
 * (pas de schéma) : un config.json à moitié écrit passerait tel quel. Dépend de `FileSystem`
 * (`@effect/platform`) plutôt que d'appeler `node:fs` en dur : un test peut fournir une
 * implémentation en mémoire sans jamais toucher `~/.aurum/config.json`. Consommateurs (App.tsx,
 * SetupScreen.tsx) : `fsRuntime.runPromise(readConfig())` (cf. src/utils/effectRuntime.ts) — l'I/O
 * de `@effect/platform-bun` est réellement async (contrairement à l'ancien `node:fs` synchrone),
 * donc `Effect.runSync` n'est plus utilisable ici (`AsyncFiberException` à l'exécution, vérifié en
 * pratique) : App.tsx charge la config dans un `useEffect`, pas dans l'initializer de `useState`.
 * Le module `Config` d'Effect cible des variables d'environnement, pas un fichier JSON chiffré sur
 * disque — pas le bon outil ici malgré le nom.
 */
export function readConfig(): Effect.Effect<AppConfig | undefined, never, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    // Une lecture fs cassée (permissions, race avec un fichier supprimé entretemps…) est traitée
    // comme "pas de config", même logique que le reste de cette fonction — un fichier illisible
    // n'est pas plus fatal qu'un fichier absent.
    const raw = yield* fs.readFileString(CONFIG_PATH).pipe(Effect.orElseSucceed(() => undefined));
    if (raw === undefined) return undefined;

    // `JSON.parse`/`decrypt` peuvent tous deux throw (JSON invalide, ciphertext corrompu/déchiffrable
    // seulement sur une autre machine) — Effect.try (pas Effect.sync) pour ne pas laisser
    // l'exception s'échapper en defect non catché.
    return yield* Effect.try(() => {
      const parsed = JSON.parse(raw) as { url: string; token: string; atrRefreshEnabled?: boolean };
      return {
        url: parsed.url,
        token: decrypt(parsed.token),
        atrRefreshEnabled: parsed.atrRefreshEnabled ?? true,
      };
    }).pipe(Effect.orElseSucceed(() => undefined));
  });
}

/** Échoue tel quel (disque plein, permissions) — contrairement à `readConfig`, un échec
 * d'écriture doit remonter à l'utilisateur (cf. SetupScreen.tsx), pas être avalé.
 *
 * `patch` plutôt qu'un `AppConfig` complet : fusionne uniquement les clés fournies par-dessus le
 * fichier existant, au lieu de le réécrire en entier. Avant ce fix, un appel qui ne voulait changer
 * que `atrRefreshEnabled` (cf. useCommandRouter.ts) aurait effacé `url`/`token` — et vice-versa, un
 * changement d'url/token depuis SetupScreen.tsx aurait effacé les réglages. Le contenu existant est
 * lu en JSON brut (pas via `readConfig`, qui déchiffre `token` — inutile ici, on ne fait que le
 * recopier tel quel si `patch.token` n'est pas fourni). */
export function writeConfig(
  patch: Partial<AppConfig>,
): Effect.Effect<void, PlatformError, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    if (!(yield* fs.exists(APP_DATA_DIR))) {
      yield* fs.makeDirectory(APP_DATA_DIR, { recursive: true });
    }

    const existingRaw = yield* fs
      .readFileString(CONFIG_PATH)
      .pipe(Effect.orElseSucceed(() => undefined));
    const existing: Record<string, unknown> =
      existingRaw === undefined
        ? {}
        : yield* Effect.try(() => JSON.parse(existingRaw) as Record<string, unknown>).pipe(
            Effect.orElseSucceed(() => ({}) as Record<string, unknown>),
          );

    const merged: Record<string, unknown> = {
      ...existing,
      ...(patch.url !== undefined ? { url: patch.url } : {}),
      ...(patch.token !== undefined ? { token: encrypt(patch.token) } : {}),
      ...(patch.atrRefreshEnabled !== undefined
        ? { atrRefreshEnabled: patch.atrRefreshEnabled }
        : {}),
    };

    yield* fs.writeFileString(CONFIG_PATH, JSON.stringify(merged, null, 2), { mode: 0o600 });
  });
}
