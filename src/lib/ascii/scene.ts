// The picture behind the hero, painted at one pixel per character cell and then turned into
// glyphs: a snow-capped mountain at sunrise above a still lake. Everything above the waterline is
// painted here in CSS pixels of the hero; the lake is the reflection of it, computed per cell by
// the backdrop (see backdrop.ts), so it can ripple.
//
// The composition is deliberately simple and iconic so it reads at a glance once in characters:
// one big peak with a lit face and a shadow face, a snow cap, a low sun beside it, a far range
// in the haze and a band of mist at the foot of the mountain.

type Point = [number, number];

/** One mountain: silhouette, the faces turned to the sun, and its snow cap. */
type Mountain = {
  shape: Path2D;
  litFaces: Path2D;
  snow: Path2D;
  snowLit: Path2D;
  /** 0 = crisp, 1 = lost in the haze. Distant mountains are paler and flatter. */
  haze: number;
};

export type Scene = {
  width: number;
  height: number;
  /** Waterline, in CSS px from the top. */
  horizon: number;
  sun: { x: number; y: number; r: number };
  farRange: Path2D;
  /** Back to front. */
  mountains: Mountain[];
};

/** Deterministic jitter in -1..1. */
function jitter(i: number, seed: number): number {
  const s = Math.sin(i * 127.1 + seed * 311.7) * 43758.5453;
  return (s - Math.floor(s)) * 2 - 1;
}

/** A jagged line between two points: midpoint displacement, a few levels deep. */
function crag(a: Point, b: Point, roughness: number, seed: number, depth = 5): Point[] {
  let points: Point[] = [a, b];
  let amount = roughness;
  for (let level = 0; level < depth; level++) {
    const next: Point[] = [];
    for (let i = 0; i < points.length - 1; i++) {
      const p = points[i] ?? a;
      const q = points[i + 1] ?? b;
      next.push(p, [
        (p[0] + q[0]) / 2 + jitter(i + level * 97, seed) * amount * 0.4,
        (p[1] + q[1]) / 2 + jitter(i + level * 53, seed + 1) * amount,
      ]);
    }
    next.push(points[points.length - 1] ?? b);
    points = next;
    amount *= 0.55;
  }
  return points;
}

function path(points: Point[], close = true): Path2D {
  const p = new Path2D();
  points.forEach(([x, y], i) => {
    if (i === 0) {
      p.moveTo(x, y);
    } else {
      p.lineTo(x, y);
    }
  });
  if (close) {
    p.closePath();
  }
  return p;
}

function createMountain(
  peak: Point,
  half: number,
  horizon: number,
  seed: number,
  haze: number,
): Mountain {
  const rise = horizon - peak[1];
  // Silhouette: left foot, left shoulder, summit, right shoulder, right foot.
  const leftFoot: Point = [peak[0] - half * 1.25, horizon];
  const leftShoulder: Point = [peak[0] - half * 0.5, peak[1] + rise * 0.42];
  const rightShoulder: Point = [peak[0] + half * 0.48, peak[1] + rise * 0.36];
  const rightFoot: Point = [peak[0] + half * 1.3, horizon];
  const roughness = rise * 0.09;
  const leftSlope = [
    ...crag(leftFoot, leftShoulder, roughness, seed + 1),
    ...crag(leftShoulder, peak, roughness, seed + 2).slice(1),
  ];
  const rightSlope = [
    ...crag(peak, rightShoulder, roughness, seed + 3),
    ...crag(rightShoulder, rightFoot, roughness, seed + 4).slice(1),
  ];
  const shape = path([
    ...leftSlope,
    ...rightSlope.slice(1),
    [rightFoot[0], horizon + 2],
    [leftFoot[0], horizon + 2],
  ]);

  // Faces turned to the sun (on the right): from each summit down a jagged spine.
  const spine = crag(peak, [peak[0] + half * 0.12, horizon], rise * 0.05, seed + 5).reverse();
  const shoulderSpine = crag(
    rightShoulder,
    [rightShoulder[0] + half * 0.05, horizon],
    rise * 0.04,
    seed + 6,
  ).reverse();
  const litFaces = new Path2D();
  litFaces.addPath(path([...rightSlope.slice(0, Math.ceil(rightSlope.length / 2)), ...spine]));
  litFaces.addPath(
    path([
      ...rightSlope.slice(Math.floor(rightSlope.length / 2)),
      [rightFoot[0], horizon],
      ...shoulderSpine,
    ]),
  );

  // Snow above a ragged line, a third of the way down.
  const snowLine = peak[1] + rise * 0.34;
  const snowEdge = crag(
    [peak[0] - half * 0.75, snowLine],
    [peak[0] + half * 0.7, snowLine + rise * 0.04],
    rise * 0.12,
    seed + 7,
    6,
  ).reverse();
  const snow = path([
    [peak[0] - half * 0.75, peak[1] - 10],
    [peak[0] + half * 0.7, peak[1] - 10],
    ...snowEdge,
  ]);
  const snowLit = path([
    peak,
    [peak[0] + half * 0.8, peak[1] - 10],
    [peak[0] + half * 0.8, snowLine + rise * 0.1],
    [peak[0] + half * 0.08, snowLine + rise * 0.1],
  ]);
  return { shape, litFaces, snow, snowLit, haze };
}

export function createScene(width: number, height: number): Scene {
  const wide = width >= 768;
  const w = width;
  const h = height;
  const horizon = h * (wide ? 0.68 : 0.8);
  const peak: Point = [w * (wide ? 0.66 : 0.55), h * (wide ? 0.2 : 0.42)];
  const half = w * (wide ? 0.34 : 0.62);
  const rise = horizon - peak[1];

  // A smaller, hazier mountain on the left, behind the main one, so that side is not empty.
  const second: Point = [w * (wide ? 0.2 : 0.12), horizon - rise * 0.7];
  const mountains = [
    createMountain(second, half * 0.7, horizon, 20, 0.2),
    createMountain(peak, half, horizon, 0, 0),
  ];

  // A low range far behind, pale with haze.
  const far = crag(
    [-20, horizon - rise * 0.12],
    [w + 20, horizon - rise * 0.18],
    rise * 0.12,
    8,
    7,
  );
  const farRange = path([...far, [w + 20, horizon + 2], [-20, horizon + 2]]);

  return {
    width,
    height,
    horizon,
    // Low to the right of the summit, clear of the right shoulder.
    sun: {
      x: peak[0] + half * (wide ? 0.78 : 0.55),
      y: peak[1] + rise * 0.14,
      r: Math.max(14, Math.min(w, h) * 0.05),
    },
    farRange,
    mountains,
  };
}

/** Paints sky, sun, far range, mountain and mist at time `t` (ms). `t = 0` is a still frame. */
export function paintScene(ctx: CanvasRenderingContext2D, scene: Scene, t: number): void {
  const { width: w, height: h, horizon, sun } = scene;

  const sky = ctx.createLinearGradient(0, 0, 0, horizon);
  sky.addColorStop(0, "#1a1d20");
  sky.addColorStop(0.5, "#3a4043");
  sky.addColorStop(0.85, "#8a8370");
  sky.addColorStop(1, "#c9a874");
  ctx.fillStyle = sky;
  ctx.fillRect(0, 0, w, horizon + 4);

  const pulse = 0.94 + 0.06 * Math.sin(t / 2600);
  const glow = ctx.createRadialGradient(sun.x, sun.y, sun.r * 0.8, sun.x, sun.y, sun.r * 9);
  glow.addColorStop(0, `rgba(246, 222, 172, ${0.75 * pulse})`);
  glow.addColorStop(0.3, `rgba(226, 190, 140, ${0.3 * pulse})`);
  glow.addColorStop(1, "rgba(226, 190, 140, 0)");
  ctx.fillStyle = glow;
  ctx.fillRect(0, 0, w, horizon + 4);
  ctx.fillStyle = "#fbf1d8";
  ctx.beginPath();
  ctx.arc(sun.x, sun.y, sun.r, 0, Math.PI * 2);
  ctx.fill();

  ctx.fillStyle = "#545a58";
  ctx.fill(scene.farRange);

  for (const mountain of scene.mountains) {
    // Distant mountains lean toward the haze colour, so they read as further away.
    const tone = (near: number, far: number) => Math.round(near + (far - near) * mountain.haze);
    const shade = (r: number, g: number, b: number) =>
      `rgb(${tone(r, 92)}, ${tone(g, 96)}, ${tone(b, 94)})`;
    ctx.fillStyle = shade(34, 36, 32);
    ctx.fill(mountain.shape);
    ctx.save();
    ctx.clip(mountain.shape);
    ctx.fillStyle = shade(138, 133, 115);
    ctx.fill(mountain.litFaces);
    ctx.fillStyle = shade(167, 175, 177);
    ctx.fill(mountain.snow);
    ctx.save();
    ctx.clip(mountain.snow);
    ctx.fillStyle = shade(241, 235, 220);
    ctx.fill(mountain.snowLit);
    ctx.restore();
    ctx.restore();
  }

  // Mist drifting along the foot of the mountain.
  for (let i = 0; i < 3; i++) {
    const cx = w * (0.25 + i * 0.3) + Math.sin(t * 0.0001 + i * 2) * w * 0.04;
    const cy = horizon - (h - horizon) * 0.1 - i * 4;
    const rx = w * 0.3;
    ctx.save();
    ctx.translate(cx, cy);
    ctx.scale(1, 0.07);
    const fog = ctx.createRadialGradient(0, 0, 0, 0, 0, rx);
    fog.addColorStop(0, "rgba(214, 206, 190, 0.38)");
    fog.addColorStop(1, "rgba(214, 206, 190, 0)");
    ctx.fillStyle = fog;
    ctx.fillRect(-rx, -rx, rx * 2, rx * 2);
    ctx.restore();
  }
}
