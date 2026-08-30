import { describe, expect, test } from "bun:test";
import { FileSystem } from "@effect/platform";
import { SystemError } from "@effect/platform/Error";
import { Effect } from "effect";
import { z } from "zod";
import { readJsonFile, writeJsonFile } from "./jsonFile.ts";

/** FS en mémoire (Map path -> contenu) plutôt qu'un vrai dossier temp sur disque : ce module n'a
 * rien de spécifique à Bun/Node, seul le contrat `FileSystem` (`@effect/platform`) compte — même
 * raison que le commentaire de `settings.ts#readConfig` ("un test peut fournir une implémentation
 * en mémoire"). `layerNoop` ne couvre que les méthodes fournies ; `readJsonFile`/`writeJsonFile`
 * n'utilisent que `readFileString`/`exists`/`makeDirectory`/`writeFileString`. */
function testFs(initialFiles: Record<string, string> = {}) {
  const files = new Map(Object.entries(initialFiles));
  const dirs = new Set<string>();
  const layer = FileSystem.layerNoop({
    readFileString: (path) => {
      const content = files.get(path);
      return content === undefined
        ? Effect.fail(
            new SystemError({
              module: "FileSystem",
              method: "readFileString",
              reason: "NotFound",
              pathOrDescriptor: path,
            }),
          )
        : Effect.succeed(content);
    },
    exists: (path) => Effect.succeed(dirs.has(path) || files.has(path)),
    makeDirectory: (path) => {
      dirs.add(path);
      return Effect.void;
    },
    writeFileString: (path, data) => {
      files.set(path, data);
      return Effect.void;
    },
  });
  return { layer, files, dirs };
}

const RecordSchema = z.object({ name: z.string(), count: z.number() });

describe("readJsonFile", () => {
  test("file absent -> undefined", async () => {
    const { layer } = testFs();
    const result = await Effect.runPromise(
      readJsonFile("/data/thing.json", RecordSchema).pipe(Effect.provide(layer)),
    );
    expect(result).toBeUndefined();
  });

  test("valid JSON matching the schema -> parsed value", async () => {
    const { layer } = testFs({ "/data/thing.json": JSON.stringify({ name: "atr", count: 3 }) });
    const result = await Effect.runPromise(
      readJsonFile("/data/thing.json", RecordSchema).pipe(Effect.provide(layer)),
    );
    expect(result).toEqual({ name: "atr", count: 3 });
  });

  test("malformed JSON -> undefined, not a thrown defect", async () => {
    const { layer } = testFs({ "/data/thing.json": "{not json" });
    const result = await Effect.runPromise(
      readJsonFile("/data/thing.json", RecordSchema).pipe(Effect.provide(layer)),
    );
    expect(result).toBeUndefined();
  });

  test("valid JSON that fails the schema (wrong type / format antérieur) -> undefined", async () => {
    const { layer } = testFs({ "/data/thing.json": JSON.stringify({ name: "atr" }) });
    const result = await Effect.runPromise(
      readJsonFile("/data/thing.json", RecordSchema).pipe(Effect.provide(layer)),
    );
    expect(result).toBeUndefined();
  });
});

describe("writeJsonFile", () => {
  test("creates the directory when missing, then writes indented JSON", async () => {
    const { layer, files, dirs } = testFs();
    await Effect.runPromise(
      writeJsonFile("/data", "/data/thing.json", { name: "atr", count: 3 }).pipe(
        Effect.provide(layer),
      ),
    );
    expect(dirs.has("/data")).toBe(true);
    expect(files.get("/data/thing.json")).toBe(JSON.stringify({ name: "atr", count: 3 }, null, 2));
  });

  test("does not recreate the directory when it already exists", async () => {
    let makeDirectoryCalls = 0;
    const layer = FileSystem.layerNoop({
      exists: () => Effect.succeed(true),
      makeDirectory: () => {
        makeDirectoryCalls++;
        return Effect.void;
      },
      writeFileString: () => Effect.void,
    });
    await Effect.runPromise(
      writeJsonFile("/data", "/data/thing.json", { ok: true }).pipe(Effect.provide(layer)),
    );
    expect(makeDirectoryCalls).toBe(0);
  });
});
