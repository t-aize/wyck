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
import { APP_DATA_DIR } from "./constants.ts";

const CONFIG_PATH = join(APP_DATA_DIR, "config.json");

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
 * redemande la config dans ces cas plutôt que planter.
 */
export function readConfig(): AppConfig | undefined {
  if (!existsSync(CONFIG_PATH)) return undefined;
  try {
    const raw = JSON.parse(readFileSync(CONFIG_PATH, "utf8"));
    if (typeof raw.url !== "string" || typeof raw.token !== "string") return undefined;
    return { url: raw.url, token: decrypt(raw.token) };
  } catch {
    return undefined;
  }
}

export function writeConfig(config: AppConfig): void {
  if (!existsSync(APP_DATA_DIR)) mkdirSync(APP_DATA_DIR, { recursive: true });
  writeFileSync(
    CONFIG_PATH,
    JSON.stringify({ url: config.url, token: encrypt(config.token) }, null, 2),
    { mode: 0o600 },
  );
}
