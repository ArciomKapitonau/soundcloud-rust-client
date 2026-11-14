import { Play, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";

export function ViewHeader({ title, subtitle, onPlayAll, onRefresh, children }: { title: string; subtitle?: ReactNode; onPlayAll?: () => void; onRefresh?: () => void; children?: ReactNode }) {
  return (
    <div className="mb-6 flex flex-wrap items-end gap-3 md:gap-4">
      <div className="min-w-[200px] flex-1">
        <h1 className="truncate text-2xl font-bold tracking-tight md:text-[28px]">{title}</h1>
        {subtitle && <div className="mt-1 text-sm text-muted">{subtitle}</div>}
      </div>
      {children}
      {onRefresh && (
        <button onClick={onRefresh} className="rounded-full p-2.5 text-muted transition hover:bg-surface-2 hover:text-fg" title="Обновить">
          <RefreshCw size={18} />
        </button>
      )}
      {onPlayAll && (
        <button onClick={onPlayAll} className="flex items-center gap-2 rounded-pill bg-accent px-5 py-2.5 text-sm font-semibold transition hover:bg-accent-hover text-on-accent">
          <Play size={16} fill="currentColor" /> Слушать
        </button>
      )}
    </div>
  );
}

export function ErrorBox({ text, onRetry }: { text: string; onRetry?: () => void }) {
  return (
    <div className="mb-4 flex items-center gap-4 rounded-lg border border-red-500/25 bg-red-500/10 px-4 py-3 text-sm text-red-200">
      <span className="flex-1">{text}</span>
      {onRetry && (
        <button onClick={onRetry} className="rounded-pill border border-red-300/30 px-3 py-1 text-xs hover:bg-red-500/20">
          Повторить
        </button>
      )}
    </div>
  );
}
