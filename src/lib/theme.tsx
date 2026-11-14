import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, errorText } from "./api";
import { applyCustomCss, applyPreset, DEFAULT_PRESET, findPreset } from "./themes";
import type { ThemeConfig } from "./types";

interface ThemeCtx {
  preset: string;
  customCss: string;
  /** Bumped on every visual change; canvas components redraw on it. */
  version: number;
  saving: boolean;
  error: string | null;
  setPreset: (id: string) => void;
  /** Applies immediately; persisted with a short debounce. */
  setCustomCss: (css: string) => void;
  resetCustomCss: () => void;
}

const Ctx = createContext<ThemeCtx | null>(null);

/**
 * Read the saved theme from Rust and apply it before React renders, so the
 * app never flashes the default theme.
 */
export async function bootstrapTheme(): Promise<ThemeConfig> {
  let cfg: ThemeConfig = { preset: DEFAULT_PRESET, customCss: "" };
  try {
    cfg = await Promise.race([
      api.getTheme(),
      new Promise<ThemeConfig>((_, rej) => setTimeout(() => rej(new Error("timeout")), 1500)),
    ]);
  } catch {
    /* keep defaults */
  }
  applyPreset(findPreset(cfg.preset).id);
  applyCustomCss(cfg.customCss);
  return cfg;
}

export function ThemeProvider({ initial, children }: { initial: ThemeConfig; children: ReactNode }) {
  const [preset, setPresetState] = useState(findPreset(initial.preset).id);
  const [customCss, setCss] = useState(initial.customCss);
  const [version, setVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const latest = useRef({ preset, customCss });
  latest.current = { preset, customCss };

  const persist = useCallback((delay: number) => {
    window.clearTimeout(timer.current);
    setSaving(true);
    timer.current = window.setTimeout(() => {
      const { preset, customCss } = latest.current;
      api
        .saveTheme(preset, customCss)
        .then(() => setError(null))
        .catch((e) => setError(errorText(e)))
        .finally(() => setSaving(false));
    }, delay);
  }, []);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const setPreset = useCallback(
    (id: string) => {
      applyPreset(id);
      setPresetState(id);
      latest.current.preset = id;
      setVersion((v) => v + 1);
      persist(0);
    },
    [persist],
  );

  const setCustomCss = useCallback(
    (css: string) => {
      applyCustomCss(css);
      setCss(css);
      latest.current.customCss = css;
      setVersion((v) => v + 1);
      persist(600);
    },
    [persist],
  );

  const resetCustomCss = useCallback(() => {
    applyCustomCss("");
    setCss("");
    latest.current.customCss = "";
    setVersion((v) => v + 1);
    persist(0);
  }, [persist]);

  const value = useMemo(
    () => ({ preset, customCss, version, saving, error, setPreset, setCustomCss, resetCustomCss }),
    [preset, customCss, version, saving, error, setPreset, setCustomCss, resetCustomCss],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useTheme(): ThemeCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useTheme outside ThemeProvider");
  return c;
}

/** Current waveform colours from the CSS variables. */
export function readWaveColors() {
  const cs = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback;
  return {
    playedFrom: v("--wave-played-from", "#ff7a1a"),
    playedTo: v("--wave-played-to", "#ff3300"),
    playedHover: v("--wave-played-hover", "#c44100"),
    hover: v("--wave-hover", "#a35a33"),
    idle: v("--wave-idle", "#5a5a5a"),
    empty: v("--wave-empty", "#333333"),
  };
}
