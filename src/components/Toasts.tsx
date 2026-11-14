import { AlertCircle, Info } from "lucide-react";
import { useApp } from "../lib/store";

export function Toasts() {
  const { toasts } = useApp();
  return (
    <div className="pointer-events-none fixed bottom-24 right-4 z-50 flex w-[min(380px,calc(100vw-2rem))] flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`pointer-events-auto flex items-start gap-3 glass rounded-lg border px-4 py-3 text-sm shadow-2xl shadow-black/60 ${
            t.kind === "error" ? "border-red-500/40 bg-surface-2 text-fg" : "border-accent/40 bg-surface-2 text-fg"
          }`}
        >
          {t.kind === "error" ? <AlertCircle size={18} className="mt-0.5 shrink-0 text-red-400" /> : <Info size={18} className="mt-0.5 shrink-0 text-accent" />}
          <span className="break-words">{t.text}</span>
        </div>
      ))}
    </div>
  );
}
