import { Check, FileCode2, Loader2, RotateCcw } from "lucide-react";
import { useTheme } from "../lib/theme";
import { CUSTOM_CSS_TEMPLATE, PRESETS, type ThemePreset } from "../lib/themes";

/** Mini mock-up of the app rendered with the preset's own colours. */
function PresetPreview({ p }: { p: ThemePreset["preview"] }) {
  const bars = [0.35, 0.6, 0.85, 0.55, 0.95, 0.7, 0.45, 0.8, 0.6, 0.4, 0.7, 0.5, 0.3, 0.55];
  return (
    <div className="flex h-[92px] overflow-hidden" style={{ background: p.bg, fontFamily: p.font, borderRadius: p.radius }}>
      <div className="flex w-6 flex-col items-center gap-1.5 pt-2" style={{ borderRight: `1px solid ${p.border}` }}>
        <span className="h-2.5 w-2.5" style={{ background: p.accent, borderRadius: p.radius }} />
        <span className="h-1 w-2.5 opacity-50" style={{ background: p.text }} />
        <span className="h-1 w-2.5 opacity-30" style={{ background: p.text }} />
      </div>
      <div className="flex flex-1 flex-col gap-1.5 p-2">
        <div className="text-[10px] font-bold leading-none" style={{ color: p.text }}>
          Aa
        </div>
        <div className="flex flex-1 gap-1.5 p-1.5" style={{ background: p.panel, border: `1px solid ${p.border}`, borderRadius: p.radius }}>
          <span className="h-full w-6 shrink-0" style={{ background: `linear-gradient(135deg, ${p.accent}, ${p.accent2})`, borderRadius: p.radius }} />
          <div className="flex flex-1 items-end gap-[2px]">
            {bars.map((h, i) => (
              <span
                key={i}
                className="w-[3px] flex-1"
                style={{ height: `${h * 100}%`, background: i < 7 ? (i % 2 ? p.accent2 : p.accent) : p.border }}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

export function AppearanceSettings() {
  const { preset, setPreset, customCss, setCustomCss, resetCustomCss, saving, error } = useTheme();

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key !== "Tab") return;
    // Tab inserts indentation instead of leaving the editor.
    e.preventDefault();
    const el = e.currentTarget;
    const { selectionStart: a, selectionEnd: b, value } = el;
    const next = value.slice(0, a) + "  " + value.slice(b);
    setCustomCss(next);
    requestAnimationFrame(() => el.setSelectionRange(a + 2, a + 2));
  };

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-[repeat(auto-fill,minmax(min(190px,100%),1fr))] gap-3">
        {PRESETS.map((p) => {
          const active = p.id === preset;
          return (
            <button
              key={p.id}
              onClick={() => setPreset(p.id)}
              aria-pressed={active}
              className={`relative rounded-lg border p-2 text-left transition ${
                active ? "border-accent bg-accent-soft" : "border-line bg-surface-2 hover:border-muted"
              }`}
            >
              <PresetPreview p={p.preview} />
              <div className="mt-2 flex items-start gap-2 px-0.5">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-semibold">{p.name}</div>
                  <div className="text-[11px] leading-snug text-dim">{p.description}</div>
                </div>
                {active && (
                  <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent text-on-accent">
                    <Check size={13} strokeWidth={3} />
                  </span>
                )}
              </div>
            </button>
          );
        })}
      </div>

      <div>
        <div className="mb-2 flex flex-wrap items-center gap-2">
          <div className="flex-1">
            <div className="text-sm font-medium">Кастомный CSS</div>
            <div className="text-xs text-dim">
              Применяется сразу поверх выбранной темы. Переменные: <code className="text-muted">--bg-primary</code>,{" "}
              <code className="text-muted">--bg-secondary</code>, <code className="text-muted">--accent</code>,{" "}
              <code className="text-muted">--text-primary</code>, <code className="text-muted">--text-secondary</code>,{" "}
              <code className="text-muted">--border-color</code>, <code className="text-muted">--radius</code>,{" "}
              <code className="text-muted">--backdrop-blur</code>, <code className="text-muted">--font-main</code> и др.
            </div>
          </div>
          <span className="flex items-center gap-1 text-xs text-dim">
            {saving ? (
              <>
                <Loader2 size={12} className="animate-spin" /> сохранение…
              </>
            ) : error ? (
              <span className="text-red-400">{error}</span>
            ) : (
              "сохранено"
            )}
          </span>
        </div>
        <textarea
          value={customCss}
          onChange={(e) => setCustomCss(e.target.value)}
          onKeyDown={onKeyDown}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          placeholder={":root {\n  --accent: #00ff66;\n}"}
          className="h-64 w-full resize-y rounded-md border border-line bg-input p-3 font-mono text-[13px] leading-relaxed text-fg outline-none transition placeholder:text-dim focus:border-accent"
        />
        <div className="mt-3 flex flex-wrap gap-2">
          <button
            onClick={resetCustomCss}
            disabled={!customCss}
            className="flex items-center gap-2 rounded-pill border border-line px-4 py-2 text-sm text-muted transition hover:border-red-400 hover:text-red-300 disabled:opacity-40"
          >
            <RotateCcw size={14} /> Сбросить кастомный CSS
          </button>
          {!customCss && (
            <button
              onClick={() => setCustomCss(CUSTOM_CSS_TEMPLATE)}
              className="flex items-center gap-2 rounded-pill border border-line px-4 py-2 text-sm text-muted transition hover:border-accent hover:text-fg"
            >
              <FileCode2 size={14} /> Вставить шаблон
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
