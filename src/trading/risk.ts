import { Effect } from "effect";
import { volumeStep } from "../instrument/specs.ts";
import { toLots } from "../utils/priceMath.ts";
import { TradeValidationError } from "./types.ts";

/**
 * P&L (devise de cotation) = Δprix × (volume / 100). Donc
 * volume = (riskAmount / stopDistance) × 100, puis snap au pas de 0.01 lot.
 */
export function computeVolume(
  riskAmount: number,
  stopDistance: number,
  lotSize: number,
): Effect.Effect<number, TradeValidationError> {
  if (stopDistance <= 0) {
    return Effect.fail(
      new TradeValidationError("Distance de stop invalide (SL identique à l'entrée ?)"),
    );
  }
  const step = volumeStep(lotSize);
  const units = riskAmount / stopDistance;
  const volume = Math.round((units * 100) / step) * step;
  if (volume < step) {
    return Effect.fail(
      new TradeValidationError(
        `Volume calculé (${toLots(volume, lotSize).toFixed(4)} lot) sous le minimum de ce compte ` +
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
