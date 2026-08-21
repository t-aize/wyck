import { describe, expect, test } from "bun:test";
import { FileSystem } from "@effect/platform";
import type { PlatformError } from "@effect/platform/Error";
import { Effect, Layer } from "effect";
import { type AppConfig, DEFAULT_ATR_SETTINGS, readConfig, writeConfig } from "./config.ts";

/**
 * `FileSystem` remplacé par une chaîne en mémoire — permet de tester la vraie logique
 * (schéma, chiffrement/déchiffrement) sans jamais toucher `~/.aurum/config.json`. Seules les 4
 * méthodes que config.ts appelle réellement sont implémentées ; le reste de l'interface
 * `FileSystem` (readFile, stream, chmod…) n'a pas besoin d'exister pour ce test — cast plutôt
 * qu'une implémentation complète, même logique que le fake `CtraderClient` de trading.test.ts.
 */
function fakeFileSystem(initial?: string) {
  let stored = initial;
  const fs = {
    readFileString: () =>
      stored === undefined
        ? Effect.fail(new Error("ENOENT") as unknown as PlatformError)
        : Effect.succeed(stored),
    writeFileString: (_path: string, data: string) =>
      Effect.sync(() => {
        stored = data;
      }),
    exists: () => Effect.succeed(true),
    makeDirectory: () => Effect.void,
  };
  const layer = Layer.succeed(FileSystem.FileSystem, fs as unknown as FileSystem.FileSystem);
  return {
    layer,
    get stored() {
      return stored;
    },
  };
}

const CONFIG: AppConfig = {
  url: "https://mcp.ctrader.com/trading/mcp",
  token: "secret-token",
  rewardRiskRatio: 1.5,
  atrMultiplier: 1.2,
  atrPeriod: 21,
  atrTimeframe: "H1",
};

describe("readConfig / writeConfig", () => {
  test("aucun fichier ⇒ undefined", async () => {
    const { layer } = fakeFileSystem(undefined);
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });

  test("round-trip : write puis read redonne le même config (le token survit chiffrement/déchiffrement)", async () => {
    const { layer } = fakeFileSystem();
    await Effect.runPromise(Effect.provide(writeConfig(CONFIG), layer));
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toEqual(CONFIG);
  });

  test("le fichier stocke le token chiffré, jamais en clair", async () => {
    // `stored` est un getter : ne pas le déstructurer avant l'écriture, il capturerait la valeur
    // (undefined) au lieu de rester une référence vivante — accéder via `fake.stored` après coup.
    const fake = fakeFileSystem();
    await Effect.runPromise(Effect.provide(writeConfig(CONFIG), fake.layer));
    expect(fake.stored).toBeDefined();
    expect(fake.stored).not.toContain(CONFIG.token);
  });

  test("JSON corrompu ⇒ undefined (pas d'exception)", async () => {
    const { layer } = fakeFileSystem("{not valid json");
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });

  test("forme valide mais url invalide (schéma) ⇒ undefined", async () => {
    const { layer } = fakeFileSystem(JSON.stringify({ url: "pas-une-url", token: "x" }));
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });

  test("champ manquant (token) ⇒ undefined", async () => {
    const { layer } = fakeFileSystem(JSON.stringify({ url: CONFIG.url }));
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });

  test("réglages ATR absents (config.json écrit avant leur ajout) ⇒ DEFAULT_ATR_SETTINGS appliqué", async () => {
    // Simule un fichier écrit par une version antérieure : même token chiffré (round-trip via
    // writeConfig, pas un texte en clair — decrypt() throw sinon), champs ATR retirés après coup.
    const fake = fakeFileSystem();
    await Effect.runPromise(Effect.provide(writeConfig(CONFIG), fake.layer));
    const { rewardRiskRatio, atrMultiplier, atrPeriod, atrTimeframe, ...legacyShape } = JSON.parse(
      fake.stored ?? "{}",
    );
    const { layer } = fakeFileSystem(JSON.stringify(legacyShape));
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toMatchObject(DEFAULT_ATR_SETTINGS);
  });

  test("token chiffré illisible (déchiffrable seulement sur une autre machine) ⇒ undefined", async () => {
    // Forme valide, mais le payload "token" n'est pas un ciphertext produit par encrypt() sur
    // cette machine — decrypt() doit throw (auth tag invalide), attrapé plutôt que planter.
    const { layer } = fakeFileSystem(
      JSON.stringify({
        url: CONFIG.url,
        token: Buffer.from("garbage-not-real-ciphertext").toString("base64"),
      }),
    );
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });
});
