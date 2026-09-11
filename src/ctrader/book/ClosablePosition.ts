import type { AmendablePosition } from "./AmendablePosition.ts";

/**
 * {@link AmendablePosition} dont `volume` API a lui aussi été confirmé —
 * prérequis pour `close_position` (le serveur exige le volume à clôturer).
 */
export type ClosablePosition = AmendablePosition & { volume: number };
