/** Deterministic hash of three integers to [0, 1). Stable across frames, so noise does not flicker. */
export function hash3(a: number, b: number, c: number): number {
  let h = (Math.imul(a, 374761393) + Math.imul(b, 668265263) + Math.imul(c, 2246822519)) | 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

export function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/** Smooth value noise in [0, 1) on a lattice of `scale` units. */
export function valueNoise(x: number, y: number, scale: number, seed: number): number {
  const gx = x / scale;
  const gy = y / scale;
  const x0 = Math.floor(gx);
  const y0 = Math.floor(gy);
  const fx = gx - x0;
  const fy = gy - y0;
  const sx = fx * fx * (3 - 2 * fx);
  const sy = fy * fy * (3 - 2 * fy);
  const top = hash3(x0, y0, seed) + (hash3(x0 + 1, y0, seed) - hash3(x0, y0, seed)) * sx;
  const bottom =
    hash3(x0, y0 + 1, seed) + (hash3(x0 + 1, y0 + 1, seed) - hash3(x0, y0 + 1, seed)) * sx;
  return top + (bottom - top) * sy;
}

export function smoothstep(edge0: number, edge1: number, value: number): number {
  const t = clamp01((value - edge0) / (edge1 - edge0));
  return t * t * (3 - 2 * t);
}
