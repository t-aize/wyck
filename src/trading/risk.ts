import { Effect } from "effect";
import { LOT_VOLUME } from "../constants.ts";
import { TradeValidationError } from "./types.ts";

// Pas/minimum de volume imposés par ce compte sur XAUUSD : 0.01 lot (confirmé via la
// plateforme du broker — pas de dropdown 0.01→1.00 lot par incréments de 0.01).
export const VOLUME_STEP = 100; // 0.01 lot

export function computeVolume(
  riskAmount: number,
  stopDistance: number,
): Effect.Effect<number, TradeValidationError> {
  if (stopDistance <= 0) {
    return Effect.fail(
      new TradeValidationError("Distance de stop invalide (SL identique à l'entrée ?)"),
    );
  }
  const ounces = riskAmount / stopDistance;
  const volume = Math.round((ounces * 100) / VOLUME_STEP) * VOLUME_STEP;
  if (volume < VOLUME_STEP) {
    return Effect.fail(
      new TradeValidationError(
        `Volume calculé (${(volume / LOT_VOLUME).toFixed(4)} lot) sous le minimum de ce compte ` +
          "(0.01 lot) — augmente le risque% ou resserre le stop",
      ),
    );
  }
  return Effect.succeed(volume);
}

export function validateRiskPercent(
  riskPercent: number,
): Effect.Effect<void, TradeValidationError> {
  if (!Number.isFinite(riskPercent) || riskPercent <= 0 || riskPercent > 100) {
    return Effect.fail(
      new TradeValidationError("Risque invalide : doit être un pourcentage entre 0 et 100"),
    );
  }
  return Effect.void;
}
