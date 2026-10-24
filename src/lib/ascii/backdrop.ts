import { ALPHA_STEPS, atlasRow, createGlyphAtlas, type GlyphAtlas, RAMP_LEVELS } from "./atlas";
import { clamp01, hash3, valueNoise } from "./hash";
import { createScene, paintScene, type Scene } from "./scene";

// Renders the hero's ASCII landscape. Framework-agnostic: the React wrapper only creates it with
// its elements and destroys it on unmount. Purely decorative: it does not react to the pointer.
//
// Each frame paints the scene at one pixel per character cell, reads the pixels back, maps each
// to a glyph code (glyph, tint, opacity) and redraws only the cells whose code changed.

export type AsciiBackdropOptions = {
  /** The hero section: sizes the backdrop. */
  root: HTMLElement;
  canvas: HTMLCanvasElement;
  fontFamily: string;
};

// Small glyphs make a fine grid: the landscape reads as an image rather than as text.
const FONT_SIZE = 6;
const LINE_HEIGHT = 7;
const FRAME_INTERVAL = 1000 / 24;
const INTRO_MS = 1800;
const BLANK = 0xffff;

export class AsciiBackdrop {
  private readonly root: HTMLElement;
  private readonly canvas: HTMLCanvasElement;
  private readonly fontFamily: string;
  private readonly ctx: CanvasRenderingContext2D | null;
  private readonly source: HTMLCanvasElement;
  private readonly sourceCtx: CanvasRenderingContext2D | null;

  private atlas: GlyphAtlas | null = null;
  private scene: Scene | null = null;
  private codes = new Uint16Array(0);
  private texture = new Float32Array(0);
  private cols = 0;
  private rows = 0;
  private dpr = 1;
  private width = 0;
  private height = 0;
  private cellWidth = 0;
  private cellHeight = 0;

  private reducedMotion: boolean;
  private visible = true;
  private ready = false;
  private destroyed = false;
  private needsResize = true;
  private frameId = 0;
  private lastFrame = 0;
  private startedAt = 0;

  private readonly motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
  private readonly resizeObserver: ResizeObserver;
  private readonly intersectionObserver: IntersectionObserver;

  constructor({ root, canvas, fontFamily }: AsciiBackdropOptions) {
    this.root = root;
    this.canvas = canvas;
    this.fontFamily = fontFamily;
    this.ctx = canvas.getContext("2d");
    this.source = document.createElement("canvas");
    this.sourceCtx = this.source.getContext("2d", { willReadFrequently: true });
    this.reducedMotion = this.motionQuery.matches;

    this.resizeObserver = new ResizeObserver(() => {
      this.needsResize = true;
      this.schedule();
    });
    this.intersectionObserver = new IntersectionObserver((entries) => {
      this.visible = entries.some((entry) => entry.isIntersecting);
      this.schedule();
    });

    this.resizeObserver.observe(root);
    this.intersectionObserver.observe(root);
    this.motionQuery.addEventListener("change", this.onMotionChange);
    document.addEventListener("visibilitychange", this.onVisibilityChange);

    void this.loadFont();
  }

  destroy(): void {
    this.destroyed = true;
    cancelAnimationFrame(this.frameId);
    this.resizeObserver.disconnect();
    this.intersectionObserver.disconnect();
    this.motionQuery.removeEventListener("change", this.onMotionChange);
    document.removeEventListener("visibilitychange", this.onVisibilityChange);
  }

  private async loadFont(): Promise<void> {
    try {
      await document.fonts.load(`${FONT_SIZE}px ${this.fontFamily}`);
    } catch {
      // The monospace fallback in the font stack still renders a usable grid.
    }
    if (this.destroyed) {
      return;
    }
    this.ready = true;
    this.startedAt = performance.now();
    this.canvas.setAttribute("data-ready", "");
    this.schedule();
  }

  private readonly onMotionChange = () => {
    this.reducedMotion = this.motionQuery.matches;
    this.codes.fill(BLANK - 1);
    this.schedule();
  };

  private readonly onVisibilityChange = () => this.schedule();

  /** Requests a frame if none is pending; the loop keeps itself alive while animating. */
  private schedule(): void {
    if (this.frameId === 0 && !this.destroyed) {
      this.frameId = requestAnimationFrame(this.frame);
    }
  }

  private readonly frame = (now: number) => {
    this.frameId = 0;
    if (!(this.ready && this.visible) || document.hidden) {
      return;
    }
    if (this.needsResize || Math.min(window.devicePixelRatio || 1, 2) !== this.dpr) {
      this.resize();
    }
    if (this.reducedMotion) {
      // One still, fully revealed frame; redrawn only on resize.
      this.draw(INTRO_MS * 2, false);
      return;
    }
    if (now - this.lastFrame >= FRAME_INTERVAL) {
      this.lastFrame = now;
      this.draw(now - this.startedAt, true);
    }
    this.schedule();
  };

  private resize(): void {
    this.needsResize = false;
    const ctx = this.ctx;
    if (!ctx) {
      return;
    }
    this.dpr = Math.min(window.devicePixelRatio || 1, 2);
    this.width = this.root.clientWidth;
    this.height = this.root.clientHeight;

    // Cells are whole device pixels so neighbouring cells never leave seams when cleared.
    const font = `${Math.round(FONT_SIZE * this.dpr)}px ${this.fontFamily}`;
    ctx.font = font;
    this.cellWidth = Math.max(1, Math.round(ctx.measureText("M").width));
    this.cellHeight = Math.round(LINE_HEIGHT * this.dpr);
    this.cols = Math.ceil((this.width * this.dpr) / this.cellWidth);
    this.rows = Math.ceil((this.height * this.dpr) / this.cellHeight);

    this.canvas.width = this.cols * this.cellWidth;
    this.canvas.height = this.rows * this.cellHeight;
    this.canvas.style.width = `${this.canvas.width / this.dpr}px`;
    this.canvas.style.height = `${this.canvas.height / this.dpr}px`;

    this.source.width = this.cols;
    this.source.height = this.rows;
    this.atlas = createGlyphAtlas(font, this.cellWidth, this.cellHeight);
    this.scene = createScene(this.width, this.height);
    this.codes = new Uint16Array(this.cols * this.rows).fill(BLANK - 1);
    ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    this.computeStaticLayers();
  }

  /** Per-cell organic texture, so flat areas of the picture are not uniformly dense. */
  private computeStaticLayers(): void {
    const { cols, rows } = this;
    const cssCellW = this.cellWidth / this.dpr;
    const cssCellH = this.cellHeight / this.dpr;
    this.texture = new Float32Array(cols * rows);
    for (let row = 0; row < rows; row++) {
      const cy = (row + 0.5) * cssCellH;
      for (let col = 0; col < cols; col++) {
        const cx = (col + 0.5) * cssCellW;
        const fbm =
          0.6 * valueNoise(cx, cy * 2.2, 110, 11) + 0.4 * valueNoise(cx, cy * 2.2, 34, 12);
        this.texture[row * cols + col] = (fbm - 0.5) * 0.14 + (hash3(col, row, 7) - 0.5) * 0.05;
      }
    }
  }

  private draw(t: number, animate: boolean): void {
    const { ctx, sourceCtx, atlas, scene } = this;
    if (!(ctx && sourceCtx && atlas && scene)) {
      return;
    }
    const { cols, rows, cellWidth, cellHeight } = this;
    const cssCellW = cellWidth / this.dpr;
    const cssCellH = cellHeight / this.dpr;

    sourceCtx.setTransform(1 / cssCellW, 0, 0, 1 / cssCellH, 0, 0);
    paintScene(sourceCtx, scene, animate ? t : 0);
    const pixels = sourceCtx.getImageData(0, 0, cols, rows).data;

    const motion = animate ? t : 0;
    const intro = clamp01(t / INTRO_MS);
    const twinkleFrame = Math.floor(motion / 650);
    const waterline = Math.round(scene.horizon / cssCellH);
    const lakeRows = Math.max(1, rows - waterline);
    const { sun } = scene;

    for (let row = 0; row < rows; row++) {
      const cy = (row + 0.5) * cssCellH;
      const rowReveal = 0.75 * (1 - row / rows);
      // Below the waterline, each row mirrors the one as far above it, shifted by a ripple.
      const depth = row - waterline;
      const lake = depth >= 0;
      const mirrorRow = waterline - 1 - depth;
      const ripple = lake
        ? Math.round(Math.sin(depth * 0.7 + motion * 0.0016) * (0.6 + depth * 0.05))
        : 0;
      const sheen = lake ? 0.82 + 0.18 * Math.sin(depth * 2.1 + motion * 0.002) : 1;
      for (let col = 0; col < cols; col++) {
        const index = row * cols + col;
        const sourceCol = Math.min(cols - 1, Math.max(0, col + ripple));
        const p = lake ? (Math.max(0, mirrorRow) * cols + sourceCol) * 4 : index * 4;
        const r = pixels[p] ?? 0;
        const g = pixels[p + 1] ?? 0;
        const b = pixels[p + 2] ?? 0;

        let lum = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 + (this.texture[index] ?? 0);

        if (lake) {
          // The reflection is darker, fades with distance, and the sun lays a path of glints.
          const fade = 1 - (depth / lakeRows) * 0.55;
          lum *= 0.58 * sheen * fade;
          const cx = (col + 0.5) * cssCellW;
          const path = Math.exp(-(((cx - sun.x) / (sun.r * (1.6 + depth * 0.06))) ** 2));
          const wave = 0.5 + 0.5 * Math.sin(depth * 1.7 + col * 0.45 + motion * 0.004);
          lum += path * wave ** 3 * 0.75 * fade;
        } else if (cy < this.height * 0.4 && hash3(col, row, 1) < 0.004) {
          // Sparse stars in the upper sky, slowly twinkling.
          lum = Math.max(lum, 0.3 + 0.45 * hash3(col, row, twinkleFrame));
        }

        lum = clamp01(lum);

        let code = BLANK;
        const revealed = intro >= 1 || intro > rowReveal + 0.25 * hash3(col, row, 3);
        const level = Math.floor(lum * (RAMP_LEVELS + 1));
        if (revealed && level > 0) {
          const glyph = Math.min(level, RAMP_LEVELS);
          const tint = lum > 0.78 ? 2 : r >= b + 6 ? 0 : 1;
          const step = Math.min(ALPHA_STEPS - 1, Math.floor(lum * ALPHA_STEPS));
          code = atlasRow(tint, step) * 64 + glyph;
        }

        if (code !== this.codes[index]) {
          this.codes[index] = code;
          const x = col * cellWidth;
          const y = row * cellHeight;
          ctx.clearRect(x, y, cellWidth, cellHeight);
          if (code !== BLANK) {
            const sx = (code % 64) * cellWidth;
            const sy = Math.floor(code / 64) * cellHeight;
            ctx.drawImage(atlas.canvas, sx, sy, cellWidth, cellHeight, x, y, cellWidth, cellHeight);
          }
        }
      }
    }
  }
}
