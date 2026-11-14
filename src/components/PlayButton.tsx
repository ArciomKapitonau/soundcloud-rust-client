import { Loader2, Pause, Play } from "lucide-react";

export function PlayButton({ state, size = 44, onClick }: { state: "playing" | "paused" | "loading"; size?: number; onClick: (e: React.MouseEvent) => void }) {
  const icon = size * 0.42;
  return (
    <button
      onClick={onClick}
      aria-label={state === "playing" ? "Пауза" : "Воспроизвести"}
      className="flex shrink-0 items-center justify-center rounded-full bg-accent text-on-accent shadow-lg shadow-black/40 transition hover:scale-105 hover:bg-accent-hover active:scale-95"
      style={{ width: size, height: size }}
    >
      {state === "loading" ? (
        <Loader2 size={icon} className="animate-spin" />
      ) : state === "playing" ? (
        <Pause size={icon} fill="currentColor" />
      ) : (
        <Play size={icon} fill="currentColor" className="translate-x-[1px]" />
      )}
    </button>
  );
}
