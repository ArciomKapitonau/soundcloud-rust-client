import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { readWaveColors, useTheme } from "../lib/theme";

const cache = new Map<string, Promise<number[]>>();

function loadWaveform(url: string): Promise<number[]> {
  let p = cache.get(url);
  if (!p) {
    p = api.waveform(url, 240).catch((e) => {
      cache.delete(url);
      throw e;
    });
    cache.set(url, p);
  }
  return p;
}

/** Deterministic placeholder for tracks without a waveform. */
function fakeWave(seed: number, n: number): number[] {
  let x = seed % 2147483647 || 1;
  return Array.from({ length: n }, (_, i) => {
    x = (x * 16807) % 2147483647;
    const env = 0.55 + 0.35 * Math.sin((i / n) * Math.PI);
    return Math.max(0.08, env * (0.6 + ((x % 1000) / 1000) * 0.4));
  });
}

interface Props {
  url: string | null;
  seed: number;
  /** 0..1 playback progress, or null when this track isn't current. */
  progress: number | null;
  onSeek?: (fraction: number) => void;
  height?: number;
  /** Draw the mirrored "reflection" under the bars, SoundCloud style. */
  reflection?: boolean;
  /** Distance between bar starts in CSS px. */
  step?: number;
}

export function Waveform({ url, seed, progress, onSeek, height = 60, reflection = true, step = 3 }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [samples, setSamples] = useState<number[] | null>(null);
  const [visible, setVisible] = useState(false);
  const [hover, setHover] = useState<number | null>(null);
  const [width, setWidth] = useState(0);
  const { version: themeVersion } = useTheme();

  // Only fetch when scrolled into view.
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const io = new IntersectionObserver(([e]) => e.isIntersecting && setVisible(true), { rootMargin: "200px" });
    io.observe(el);
    const ro = new ResizeObserver(([e]) => setWidth(e.contentRect.width));
    ro.observe(el);
    return () => {
      io.disconnect();
      ro.disconnect();
    };
  }, []);

  useEffect(() => {
    if (!visible) return;
    if (!url) {
      setSamples(fakeWave(seed, 240));
      return;
    }
    let alive = true;
    loadWaveform(url)
      .then((s) => alive && setSamples(s))
      .catch(() => alive && setSamples(fakeWave(seed, 240)));
    return () => {
      alive = false;
    };
  }, [url, seed, visible]);

  useEffect(() => {
    const c = canvas.current;
    if (!c || !width) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.floor(width * dpr);
    c.height = Math.floor(height * dpr);
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);

    const n = Math.max(1, Math.floor(width / step));
    const src = samples ?? new Array(240).fill(0.06);
    const barW = Math.max(1, step - 1);
    const topH = reflection ? height * 0.7 : height;
    const botH = height - topH - 1;
    const played = progress ?? 0;
    const hov = hover;

    const col = readWaveColors();
    const grad = ctx.createLinearGradient(0, 0, 0, topH);
    grad.addColorStop(0, col.playedFrom);
    grad.addColorStop(1, col.playedTo);

    for (let i = 0; i < n; i++) {
      const a = Math.floor((i / n) * src.length);
      const b = Math.max(a + 1, Math.floor(((i + 1) / n) * src.length));
      let v = 0;
      for (let k = a; k < b && k < src.length; k++) v = Math.max(v, src[k]);
      const x = i * step;
      const frac = (i + 0.5) / n;
      const h = Math.max(1, v * topH);
      const isPlayed = progress !== null && frac <= played;
      const isHover = hov !== null && frac <= hov;

      let top: string | CanvasGradient;
      let bot: string;
      if (isPlayed && isHover) {
        top = bot = col.playedHover;
      } else if (isPlayed) {
        top = grad;
        bot = col.playedTo;
      } else if (isHover) {
        top = bot = col.hover;
      } else {
        top = bot = samples ? col.idle : col.empty;
      }
      ctx.globalAlpha = 1;
      ctx.fillStyle = top;
      ctx.fillRect(x, topH - h, barW, h);
      if (reflection) {
        // Lighter "reflection" under the bars, SoundCloud style.
        ctx.globalAlpha = 0.45;
        ctx.fillStyle = bot;
        ctx.fillRect(x, topH + 1, barW, Math.max(1, v * botH));
      }
    }
  }, [samples, width, height, progress, hover, reflection, step, themeVersion]);

  const fractionAt = (clientX: number) => {
    const r = wrap.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width));
  };

  return (
    <div
      ref={wrap}
      className={`relative w-full ${onSeek ? "cursor-pointer" : ""}`}
      style={{ height }}
      onMouseMove={onSeek ? (e) => setHover(fractionAt(e.clientX)) : undefined}
      onMouseLeave={() => setHover(null)}
      onClick={
        onSeek
          ? (e) => {
              e.stopPropagation();
              onSeek(fractionAt(e.clientX));
            }
          : undefined
      }
    >
      <canvas ref={canvas} style={{ width: "100%", height }} className="block" />
    </div>
  );
}
