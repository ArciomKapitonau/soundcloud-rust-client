/**
 * Built-in theme presets.
 *
 * A preset is a set of CSS custom-property overrides (see the defaults in
 * src/index.css) plus optional extra CSS for effects that variables can't
 * express (glitch, neon glow, grain…). Presets are injected into
 * <style id="theme-preset">; the user's CSS goes into
 * <style id="user-custom-theme"> after it, so it can override anything.
 */

export interface ThemePreset {
  id: string;
  name: string;
  description: string;
  /** Colours for the preview card in Settings. */
  preview: { bg: string; panel: string; accent: string; accent2: string; text: string; border: string; font: string; radius: string };
  vars: Record<string, string>;
  extraCss?: string;
}

const MONO = `"JetBrainsMono Nerd Font", "JetBrains Mono", "Fira Code", ui-monospace, "DejaVu Sans Mono", monospace`;
const SANS = `"Inter", ui-sans-serif, system-ui, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif`;

export const PRESETS: ThemePreset[] = [
  {
    id: "soundcloud-dark",
    name: "SoundCloud Dark",
    description: "Классика: #121212 и фирменный оранжевый",
    preview: { bg: "#121212", panel: "#1c1c1c", accent: "#ff5500", accent2: "#ff3300", text: "#ffffff", border: "#2e2e2e", font: SANS, radius: "8px" },
    vars: {}, // defaults from index.css
  },
  {
    id: "cyberpunk",
    name: "Cyberpunk / Phonk",
    description: "Неоновый фиолетовый и кислотно-зелёный",
    preview: { bg: "#0a0a0f", panel: "#14141f", accent: "#b026ff", accent2: "#00ff66", text: "#e8e6ff", border: "#2a2440", font: SANS, radius: "6px" },
    vars: {
      "--bg-app": "radial-gradient(1200px 600px at 100% 0%, rgb(176 38 255 / 0.10), transparent 60%), radial-gradient(900px 500px at 0% 100%, rgb(0 255 102 / 0.05), transparent 60%), #0a0a0f",
      "--bg-primary": "#0a0a0f",
      "--bg-secondary": "rgb(20 20 31 / 0.85)",
      "--bg-tertiary": "#1a1a2a",
      "--bg-elevated": "#26263c",
      "--bg-sidebar": "rgb(7 7 11 / 0.92)",
      "--bg-player": "rgb(10 10 15 / 0.88)",
      "--bg-input": "#06060a",
      "--accent": "#b026ff",
      "--accent-2": "#00ff66",
      "--on-accent": "#ffffff",
      "--text-primary": "#ece9ff",
      "--text-secondary": "#9d97cc",
      "--text-muted": "#5f5a84",
      "--border-color": "#2a2440",
      "--card-border": "rgb(176 38 255 / 0.16)",
      "--card-border-hover": "rgb(0 255 102 / 0.55)",
      "--radius": "6px",
      "--glow": "0 0 0 1px rgb(0 255 102 / 0.25), 0 0 24px rgb(176 38 255 / 0.35)",
      "--heading-transform": "uppercase",
      "--heading-spacing": "0.06em",
      "--wave-played-from": "#00ff66",
      "--wave-played-to": "#b026ff",
      "--wave-played-hover": "#7a1bb3",
      "--wave-hover": "#5b3a7a",
      "--wave-idle": "#3d3a5c",
      "--wave-empty": "#22203a",
    },
    extraCss: `
h1 { text-shadow: 0 0 12px rgb(176 38 255 / 0.65), 0 0 2px rgb(0 255 102 / 0.6); }
.bg-accent { box-shadow: 0 0 18px rgb(176 38 255 / 0.55); }
`,
  },
  {
    id: "sigilkore",
    name: "Sigilkore / Y2K Trap",
    description: "OLED-чёрный, ядовито-розовый, моноширинный, глитч",
    preview: { bg: "#050505", panel: "#0d0d0d", accent: "#ff007f", accent2: "#00ff66", text: "#f2f2f2", border: "#262626", font: MONO, radius: "2px" },
    vars: {
      "--bg-app": "#050505",
      "--bg-primary": "#050505",
      "--bg-secondary": "#0c0c0c",
      "--bg-tertiary": "#141414",
      "--bg-elevated": "#1e1e1e",
      "--bg-sidebar": "#000000",
      "--bg-player": "rgb(0 0 0 / 0.94)",
      "--bg-input": "#000000",
      "--accent": "#ff007f",
      "--accent-2": "#00ff66",
      "--on-accent": "#000000",
      "--text-primary": "#f2f2f2",
      "--text-secondary": "#a3a3a3",
      "--text-muted": "#5e5e5e",
      "--border-color": "#262626",
      "--card-border": "#1c1c1c",
      "--card-border-hover": "#ff007f",
      "--radius": "2px",
      "--radius-pill": "2px",
      "--font-main": MONO,
      "--heading-transform": "lowercase",
      "--heading-spacing": "0.02em",
      "--wave-played-from": "#ff007f",
      "--wave-played-to": "#00ff66",
      "--wave-played-hover": "#b3005a",
      "--wave-hover": "#5c1f3d",
      "--wave-idle": "#3a3a3a",
      "--wave-empty": "#1c1c1c",
    },
    extraCss: `
@keyframes sc-glitch {
  0%   { transform: translate(0); clip-path: inset(0 0 0 0); }
  20%  { transform: translate(-2px, 1px); clip-path: inset(10% 0 60% 0); }
  40%  { transform: translate(2px, -1px); clip-path: inset(50% 0 20% 0); }
  60%  { transform: translate(-1px, 0); clip-path: inset(30% 0 40% 0); }
  80%  { transform: translate(1px, 1px); clip-path: inset(70% 0 5% 0); }
  100% { transform: translate(0); clip-path: inset(0 0 0 0); }
}
@keyframes sc-glitch-shadow {
  0%, 100% { text-shadow: none; }
  25% { text-shadow: -2px 0 #ff007f, 2px 0 #00ff66; }
  50% { text-shadow: 2px 0 #ff007f, -2px 0 #00ff66; }
  75% { text-shadow: -1px 1px #ff007f, 1px -1px #00ff66; }
}
.track-card:hover { animation: sc-glitch 0.32s steps(2, end) 1; }
.track-card:hover .text-fg { animation: sc-glitch-shadow 0.45s steps(3, end) 2; }
h1::before { content: "†"; color: var(--accent); margin-right: 0.35em; }
`,
  },
  {
    id: "hyprland",
    name: "Hyprland / Rice",
    description: "Catppuccin Mocha, стекло, рамки 1px, радиус 4px",
    preview: { bg: "rgba(30,30,46,0.85)", panel: "rgba(49,50,68,0.7)", accent: "#89b4fa", accent2: "#cba6f7", text: "#cdd6f4", border: "#45475a", font: MONO, radius: "4px" },
    vars: {
      "--bg-app": "rgb(30 30 46 / 0.80)",
      "--bg-primary": "rgb(30 30 46 / 0.80)",
      "--bg-secondary": "rgb(49 50 68 / 0.45)",
      "--bg-tertiary": "rgb(69 71 90 / 0.50)",
      "--bg-elevated": "rgb(88 91 112 / 0.60)",
      "--bg-sidebar": "rgb(24 24 37 / 0.60)",
      "--bg-player": "rgb(17 17 27 / 0.65)",
      "--bg-input": "rgb(17 17 27 / 0.55)",
      "--accent": "#89b4fa",
      "--accent-2": "#cba6f7",
      "--accent-hover": "#b4cffc",
      "--on-accent": "#1e1e2e",
      "--text-primary": "#cdd6f4",
      "--text-secondary": "#a6adc8",
      "--text-muted": "#7f849c",
      "--border-color": "#45475a",
      "--card-border": "#45475a",
      "--card-border-hover": "#89b4fa",
      "--radius": "4px",
      "--radius-pill": "4px",
      "--backdrop-blur": "12px",
      "--font-main": MONO,
      "--heading-spacing": "0",
      "--wave-played-from": "#cba6f7",
      "--wave-played-to": "#89b4fa",
      "--wave-played-hover": "#7287fd",
      "--wave-hover": "#6c7086",
      "--wave-idle": "#585b70",
      "--wave-empty": "#45475a",
    },
    extraCss: `
.track-card, .themed-card { backdrop-filter: blur(var(--backdrop-blur)); }
.track-card:hover { border-image: linear-gradient(45deg, #89b4fa, #cba6f7) 1; }
`,
  },
  {
    id: "gachiakuta",
    name: "Gachiakuta / Grime",
    description: "Чернильно-чёрный, ржавчина, грубые рамки",
    preview: { bg: "#0b0b0c", panel: "#141415", accent: "#cc3300", accent2: "#ff6a1a", text: "#eee8dc", border: "#3a3633", font: `Impact, "Anton", "Bebas Neue", sans-serif`, radius: "0px" },
    vars: {
      "--bg-app": "#0b0b0c",
      "--bg-primary": "#0b0b0c",
      "--bg-secondary": "#131314",
      "--bg-tertiary": "#1b1b1c",
      "--bg-elevated": "#2a2928",
      "--bg-sidebar": "#070708",
      "--bg-player": "rgb(9 9 10 / 0.97)",
      "--bg-input": "#050506",
      "--accent": "#cc3300",
      "--accent-2": "#ff6a1a",
      "--accent-hover": "#e64a14",
      "--on-accent": "#f5efe6",
      "--text-primary": "#eee8dc",
      "--text-secondary": "#a59f93",
      "--text-muted": "#6b665e",
      "--border-color": "#3a3633",
      "--card-border": "#2e2b28",
      "--card-border-hover": "#cc3300",
      "--border-width": "2px",
      "--radius": "0px",
      "--radius-pill": "0px",
      "--glow": "5px 5px 0 #000, 5px 5px 0 2px #cc3300",
      "--font-display": `Impact, "Anton", "Bebas Neue", "Oswald", Haettenschweiler, "Arial Narrow Bold", sans-serif`,
      "--heading-transform": "uppercase",
      "--heading-spacing": "0.04em",
      "--wave-played-from": "#ff6a1a",
      "--wave-played-to": "#8f2400",
      "--wave-played-hover": "#7a1f00",
      "--wave-hover": "#5a3328",
      "--wave-idle": "#4a4642",
      "--wave-empty": "#2a2826",
    },
    extraCss: `
body::after {
  content: ""; position: fixed; inset: 0; pointer-events: none; z-index: 60; opacity: 0.07; mix-blend-mode: overlay;
  background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='160' height='160'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='3' stitchTiles='stitch'/></filter><rect width='100%' height='100%' filter='url(%23n)'/></svg>");
}
.track-card:hover { transform: translate(-2px, -2px); }
h1 { border-left: 6px solid var(--accent); padding-left: 12px; }
`,
  },
  {
    id: "lolicore",
    name: "Lolicore / Vapor Glitch",
    description: "Пастельные градиенты и мягкое неоновое свечение",
    preview: { bg: "linear-gradient(135deg,#1d1526,#131a26)", panel: "rgba(255,183,197,0.10)", accent: "#ffb7c5", accent2: "#b5e2fa", text: "#fff4f8", border: "rgba(255,183,197,0.3)", font: SANS, radius: "14px" },
    vars: {
      "--bg-app": "radial-gradient(900px 600px at 0% 0%, rgb(255 183 197 / 0.16), transparent 60%), radial-gradient(900px 600px at 100% 100%, rgb(181 226 250 / 0.14), transparent 60%), linear-gradient(135deg, #1d1526, #131a26)",
      "--bg-primary": "#18131f",
      "--bg-secondary": "rgb(255 255 255 / 0.05)",
      "--bg-tertiary": "rgb(255 183 197 / 0.10)",
      "--bg-elevated": "rgb(181 226 250 / 0.16)",
      "--bg-sidebar": "rgb(24 18 32 / 0.55)",
      "--bg-player": "rgb(24 18 32 / 0.65)",
      "--bg-input": "rgb(0 0 0 / 0.25)",
      "--accent": "#ffb7c5",
      "--accent-2": "#b5e2fa",
      "--accent-hover": "#ffd0da",
      "--on-accent": "#2a1830",
      "--text-primary": "#fff4f8",
      "--text-secondary": "#d6c3d8",
      "--text-muted": "#8f809b",
      "--border-color": "rgb(255 183 197 / 0.20)",
      "--card-border": "rgb(181 226 250 / 0.14)",
      "--card-border-hover": "rgb(255 183 197 / 0.6)",
      "--radius": "14px",
      "--backdrop-blur": "10px",
      "--glow": "0 0 24px rgb(255 183 197 / 0.35), 0 0 48px rgb(181 226 250 / 0.15)",
      "--font-main": `"Nunito", "Quicksand", "Varela Round", ${SANS}`,
      "--wave-played-from": "#ffb7c5",
      "--wave-played-to": "#b5e2fa",
      "--wave-played-hover": "#e891a6",
      "--wave-hover": "#8a6f86",
      "--wave-idle": "#5e5470",
      "--wave-empty": "#3a3348",
    },
    extraCss: `
.bg-accent { background-image: linear-gradient(135deg, #ffb7c5, #b5e2fa); box-shadow: 0 0 16px rgb(255 183 197 / 0.55); }
h1 { background: linear-gradient(90deg, #ffb7c5, #b5e2fa); -webkit-background-clip: text; background-clip: text; color: transparent; filter: drop-shadow(0 0 10px rgb(255 183 197 / 0.4)); }
.track-card, .themed-card { backdrop-filter: blur(var(--backdrop-blur)); }
`,
  },
];

export const DEFAULT_PRESET = PRESETS[0].id;

export function findPreset(id: string): ThemePreset {
  return PRESETS.find((p) => p.id === id) ?? PRESETS[0];
}

function styleEl(id: string): HTMLStyleElement {
  let el = document.getElementById(id) as HTMLStyleElement | null;
  if (!el) {
    el = document.createElement("style");
    el.id = id;
    document.head.appendChild(el);
  }
  return el;
}

/** Inject a preset. Order matters: preset first, user CSS after it. */
export function applyPreset(id: string) {
  const p = findPreset(id);
  const vars = Object.entries(p.vars)
    .map(([k, v]) => `  ${k}: ${v};`)
    .join("\n");
  styleEl("theme-preset").textContent = `:root {\n${vars}\n}\n${p.extraCss ?? ""}`;
  document.documentElement.dataset.theme = p.id;
  // Keep the user stylesheet last so it wins the cascade.
  const user = document.getElementById("user-custom-theme");
  if (user) document.head.appendChild(user);
}

export function applyCustomCss(css: string) {
  styleEl("user-custom-theme").textContent = css;
}

export const CUSTOM_CSS_TEMPLATE = `/* Переопределите любые переменные темы */
:root {
  --accent: #ff5500;
  --accent-2: #ff3300;
  --bg-primary: #121212;
  --bg-secondary: #181818;
  --text-primary: #ffffff;
  --text-secondary: #9a9a9a;
  --border-color: #2e2e2e;
  --radius: 8px;
  --backdrop-blur: 8px;
  --font-main: "Inter", sans-serif;
}

/* …или любые селекторы */
.track-card:hover {
  transform: translateY(-2px);
}
`;
