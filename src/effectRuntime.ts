/**
 * Point de composition unique pour la `Layer` `FileSystem` (config, cache calendrier) — la doc
 * Effect est explicite : un seul `Effect.provide`/runtime par appli, pas un `Effect.provide(...,
 * BunFileSystem.layer)` ad hoc à chaque site d'appel (App.tsx, SetupScreen.tsx, useCalendar.ts
 * faisaient chacun le leur avant ce fichier). `ManagedRuntime` (pas juste `Layer`) parce que ces
 * Effects s'exécutent hors d'un composant React (pas de hook à monter/démonter autour) — même choix
 * que `runtime` dans App.tsx pour `CtraderClient`.
 */

import { BunFileSystem } from "@effect/platform-bun";
import { ManagedRuntime } from "effect";

export const fsRuntime = ManagedRuntime.make(BunFileSystem.layer);
