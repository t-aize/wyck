/**
 * Config utilisateur (URL + token MCP cTrader), persistée dans le homedir plutôt que
 * dans `.env` : un `.exe` compilé (`bun build --compile`) n'embarque pas `.env` et peut
 * être lancé depuis n'importe quel dossier (double-clic), donc `process.cwd()` au
 * runtime n'a aucune raison de contenir le fichier — la config doit vivre à un endroit
 * stable indépendant du dossier de lancement. dev et .exe lisent donc la même chose ici.
 */

import { createCipheriv, createDecipheriv, randomBytes, scryptSync } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { hostname, userInfo } from "node:os";
import { join } from "node:path";
import { Context, Effect, Layer } from "effect";
import { z } from "zod";
import { APP_DATA_DIR } from "./constants.ts";

const CONFIG_PATH = join(APP_DATA_DIR, "config.json");

/**
 * Le module `Config` d'Effect cible des variables d'environnement, pas un fichier JSON chiffré
 * sur disque — pas le bon outil ici malgré le nom. Le vrai point d'injection utile (cf.
 * AUDIT_EFFECT.md §4.2) est la lecture/écriture du fichier lui-même : `readConfig`/`writeConfig`
 * gardent toute la logique métier (schéma, chiffrement) en clair, testable directement, et ne
 * dépendent que de ce petit service pour le fs — un test peut fournir un `ConfigFileIO` en
 * mémoire sans jamais toucher `~/.aurum/config.json` ni monkey-patcher `node:fs`.
 */
export class ConfigFileIO extends Context.Tag("ConfigFileIO")<
  ConfigFileIO,
  {
    /** N'échoue jamais : une lecture fs cassée (permissions, race avec un fichier supprimé entre
     * existsSync/readFileSync…) est traitée comme "pas de config", même logique que le reste de
     * readConfig() plus bas — un fichier illisible n'est pas plus fatal qu'un fichier absent. */
    readonly read: Effect.Effect<string | undefined>;
    /** Peut échouer (disque plein, permissions) — propagé tel quel, contrairement à `read` : un
     * échec d'écriture doit remonter à l'utilisateur (cf. SetupScreen.tsx), pas être avalé. */
    readonly write: (content: string) => Effect.Effect<void, unknown>;
  }
>() {}

export const ConfigFileIOLive = Layer.succeed(ConfigFileIO, {
  read: Effect.try(() =>
    existsSync(CONFIG_PATH) ? readFileSync(CONFIG_PATH, "utf8") : undefined,
  ).pipe(Effect.orElseSucceed(() => undefined)),
  write: (content: string) =>
    Effect.try(() => {
      if (!existsSync(APP_DATA_DIR)) mkdirSync(APP_DATA_DIR, { recursive: true });
      writeFileSync(CONFIG_PATH, content, { mode: 0o600 });
    }),
});

/** Source unique de vérité pour ce qui constitue une config valide — réutilisé par SetupScreen.tsx
 * pour valider la saisie utilisateur, pour que les deux points d'entrée (saisie, fichier relu)
 * s'accordent par construction plutôt que par coïncidence (cf. AUDIT_EFFECT.md §5.3). */
export const AppConfigSchema = z.object({
  url: z.string().url(),
  token: z.string().min(1),
});

export interface AppConfig {
  url: string;
  token: string;
}

// ponytail: clé dérivée de la machine/l'utilisateur (pas de dépendance keychain
// cross-platform genre keytar). Ça évite le token en clair dans le fichier — protège
// contre une lecture accidentelle (cat, backup cloud, capture d'écran) — mais pas
// contre un attaquant qui a déjà un accès complet à la session de cet utilisateur (il
// peut recalculer la même clé). Passer à DPAPI/keychain natif si ce niveau ne suffit pas.
function deriveKey(): Buffer {
  return scryptSync(`${hostname()}:${userInfo().username}:aurum-config`, "aurum-config-salt", 32);
}

function encrypt(text: string): string {
  const iv = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", deriveKey(), iv);
  const ciphertext = Buffer.concat([cipher.update(text, "utf8"), cipher.final()]);
  return Buffer.concat([iv, cipher.getAuthTag(), ciphertext]).toString("base64");
}

function decrypt(payload: string): string {
  const raw = Buffer.from(payload, "base64");
  const iv = raw.subarray(0, 12);
  const authTag = raw.subarray(12, 28);
  const ciphertext = raw.subarray(28);
  const decipher = createDecipheriv("aes-256-gcm", deriveKey(), iv);
  decipher.setAuthTag(authTag);
  return Buffer.concat([decipher.update(ciphertext), decipher.final()]).toString("utf8");
}

/**
 * `undefined` si absent, corrompu, incomplet, ou déchiffrable seulement sur une autre machine —
 * redemande la config dans ces cas plutôt que planter. Consommateurs (App.tsx, SetupScreen.tsx) :
 * `Effect.runSync(Effect.provide(readConfig(), ConfigFileIOLive))` — reste synchrone comme avant,
 * seule l'origine du fs devient substituable.
 */
export function readConfig(): Effect.Effect<AppConfig | undefined, never, ConfigFileIO> {
  return Effect.gen(function* () {
    const io = yield* ConfigFileIO;
    const raw = yield* io.read;
    if (raw === undefined) return undefined;

    // `JSON.parse`/`decrypt` peuvent tous deux throw (JSON invalide, ciphertext corrompu/déchiffrable
    // seulement sur une autre machine) — Effect.try (pas Effect.sync) pour ne pas laisser
    // l'exception s'échapper en defect non catché.
    return yield* Effect.try(() => {
      const parsed = AppConfigSchema.safeParse(JSON.parse(raw));
      if (!parsed.success) return undefined;
      return { url: parsed.data.url, token: decrypt(parsed.data.token) };
    }).pipe(Effect.orElseSucceed(() => undefined));
  });
}

export function writeConfig(config: AppConfig): Effect.Effect<void, unknown, ConfigFileIO> {
  return Effect.gen(function* () {
    const io = yield* ConfigFileIO;
    yield* io.write(JSON.stringify({ url: config.url, token: encrypt(config.token) }, null, 2));
  });
}
