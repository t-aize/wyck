// Every glyph the backdrop can show, pre-drawn once in each tint and opacity. Frames then copy
// cells out of this atlas with drawImage, which is far cheaper than fillText per character.

/** Index 0 is blank. Indices 1..RAMP_LEVELS map brightness to ink, sparse to dense. */
export const GLYPHS = " .,:;~=+*#%@";
export const RAMP_LEVELS = 11;

export const TINTS = [
  [214, 196, 160], // sand: sunlit sky and rims
  [168, 182, 186], // slate: cool sky and far ridges
  [236, 231, 218], // paper: the brightest cells
] as const;

export const ALPHA_STEPS = 8;

export type GlyphAtlas = {
  canvas: HTMLCanvasElement;
  cellWidth: number;
  cellHeight: number;
};

export function alphaForStep(step: number): number {
  return 0.14 + (step / (ALPHA_STEPS - 1)) * 0.8;
}

/** Atlas row for a tint and opacity step; column = glyph index. */
export function atlasRow(tint: number, step: number): number {
  return tint * ALPHA_STEPS + step;
}

export function createGlyphAtlas(font: string, cellWidth: number, cellHeight: number): GlyphAtlas {
  const canvas = document.createElement("canvas");
  canvas.width = GLYPHS.length * cellWidth;
  canvas.height = TINTS.length * ALPHA_STEPS * cellHeight;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    ctx.font = font;
    ctx.textBaseline = "middle";
    ctx.textAlign = "center";
    TINTS.forEach(([r, g, b], tint) => {
      for (let step = 0; step < ALPHA_STEPS; step++) {
        ctx.fillStyle = `rgba(${r}, ${g}, ${b}, ${alphaForStep(step)})`;
        const y = atlasRow(tint, step) * cellHeight + cellHeight / 2;
        for (let i = 1; i < GLYPHS.length; i++) {
          ctx.fillText(GLYPHS.charAt(i), i * cellWidth + cellWidth / 2, y);
        }
      }
    });
  }
  return { canvas, cellWidth, cellHeight };
}
