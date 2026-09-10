import type { CtraderPosition } from "./CtraderPosition.ts";

/**
 * {@link CtraderPosition} dont `id` a été confirmé résolu — prérequis pour
 * `amend_position` (le serveur exige `positionId`).
 */
export type AmendablePosition = CtraderPosition & { id: number };
