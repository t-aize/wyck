import { describe, expect, test } from "bun:test";
import { Effect, Layer } from "effect";
import { type AppConfig, ConfigFileIO, readConfig, writeConfig } from "./config.ts";

/** IO fs remplacé par une chaîne en mémoire — permet de tester la vraie logique
 * (schéma, chiffrement/déchiffrement) sans jamais toucher ~/.aurum/config.json. */
function fakeConfigFileIO(initial?: string) {
  let stored = initial;
  const layer = Layer.succeed(ConfigFileIO, {
    read: Effect.sync(() => stored),
    write: (content: string) =>
      Effect.sync(() => {
        stored = content;
      }),
  });
  return {
    layer,
    get stored() {
      return stored;
    },
  };
}

const CONFIG: AppConfig = { url: "https://mcp.ctrader.com/trading/mcp", token: "secret-token" };

describe("readConfig / writeConfig", () => {
  test("aucun fichier ⇒ undefined", async () => {
    const { layer } = fakeConfigFileIO(undefined);
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });

  test("round-trip : write puis read redonne le même config (le token survit chiffrement/déchiffrement)", async () => {
    const { layer } = fakeConfigFileIO();
    await Effect.runPromise(Effect.provide(writeConfig(CONFIG), layer));
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toEqual(CONFIG);
  });

  test("le fichier stocke le token chiffré, jamais en clair", async () => {
    // `stored` est un getter : ne pas le déstructurer avant l'écriture, il capturerait la valeur
    // (undefined) au lieu de rester une référence vivante — accéder via `fake.stored` après coup.
    const fake = fakeConfigFileIO();
    await Effect.runPromise(Effect.provide(writeConfig(CONFIG), fake.layer));
    expect(fake.stored).toBeDefined();
    expect(fake.stored).not.toContain(CONFIG.token);
  });

  test("JSON corrompu ⇒ undefined (pas d'exception)", async () => {
    const { layer } = fakeConfigFileIO("{not valid json");
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });

  test("forme valide mais url invalide (schéma) ⇒ undefined", async () => {
    const { layer } = fakeConfigFileIO(JSON.stringify({ url: "pas-une-url", token: "x" }));
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });

  test("champ manquant (token) ⇒ undefined", async () => {
    const { layer } = fakeConfigFileIO(JSON.stringify({ url: CONFIG.url }));
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });

  test("token chiffré illisible (déchiffrable seulement sur une autre machine) ⇒ undefined", async () => {
    // Forme valide, mais le payload "token" n'est pas un ciphertext produit par encrypt() sur
    // cette machine — decrypt() doit throw (auth tag invalide), attrapé plutôt que planter.
    const { layer } = fakeConfigFileIO(
      JSON.stringify({
        url: CONFIG.url,
        token: Buffer.from("garbage-not-real-ciphertext").toString("base64"),
      }),
    );
    const result = await Effect.runPromise(Effect.provide(readConfig(), layer));
    expect(result).toBeUndefined();
  });
});
