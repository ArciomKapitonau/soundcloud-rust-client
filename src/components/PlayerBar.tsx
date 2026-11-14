import { useEffect, useState } from "react";
import { ExternalLink, Heart, Repeat, Repeat1, SkipBack, SkipForward, Volume1, Volume2, VolumeX } from "lucide-react";
import { useApp } from "../lib/store";
import { api } from "../lib/api";
import { formatTime } from "../lib/format";
import type { RepeatMode } from "../lib/types";
import { Cover } from "./Cover";
import { PlayButton } from "./PlayButton";
import { Waveform } from "./Waveform";

const nextRepeat: Record<RepeatMode, RepeatMode> = { off: "all", all: "one", one: "off" };

export function PlayerBar() {
  const { player, likedIds, toggleLike, seekFraction } = useApp();
  const t = player.track;
  const [volume, setVolume] = useState(player.volume);
  const [muted, setMuted] = useState<number | null>(null);

  useEffect(() => setVolume(player.volume), [player.volume]);

  const changeVolume = (v: number) => {
    setVolume(v);
    api.setVolume(v);
  };
  const toggleMute = () => {
    if (muted !== null) {
      changeVolume(muted || 0.5);
      setMuted(null);
    } else {
      setMuted(volume);
      changeVolume(0);
    }
  };

  const progress = player.duration > 0 ? Math.min(1, player.position / player.duration) : 0;
  const liked = t ? likedIds.has(t.id) : false;
  const VolIcon = volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;
  const playState = player.status === "playing" ? "playing" : player.status === "loading" ? "loading" : "paused";

  return (
    <footer className="flex h-[76px] shrink-0 items-center gap-3 glass border-t border-line bg-player px-3 md:h-[84px] md:gap-6 md:px-4">
      {/* Track info */}
      <div className="flex min-w-0 shrink-0 items-center gap-3 sm:w-[clamp(140px,24vw,280px)]">
        {t ? (
          <>
            <Cover src={t.artworkUrl} alt={t.title} className="h-12 w-12 shrink-0 md:h-14 md:w-14" />
            <div className="hidden min-w-0 flex-1 sm:block">
              <div className="truncate text-sm font-semibold" title={t.title}>
                {t.title}
              </div>
              <div className="truncate text-xs text-muted">{t.artist}</div>
              {player.preview && <div className="text-[10px] font-semibold uppercase text-accent">Превью 30 сек</div>}
            </div>
            <button
              onClick={() => toggleLike(t)}
              className={`hidden rounded-full p-2 transition hover:bg-surface-3 md:block ${liked ? "text-accent" : "text-muted hover:text-fg"}`}
              title={liked ? "Убрать лайк" : "Нравится"}
            >
              <Heart size={18} fill={liked ? "currentColor" : "none"} />
            </button>
          </>
        ) : (
          <div className="hidden text-sm text-dim sm:block">Ничего не играет</div>
        )}
      </div>

      {/* Transport */}
      <div className="flex shrink-0 items-center gap-1 md:gap-2">
        <button onClick={() => api.prev()} disabled={!t} className="rounded-full p-2 text-muted transition hover:text-fg disabled:opacity-40" title="Предыдущий">
          <SkipBack size={20} fill="currentColor" />
        </button>
        <PlayButton state={playState} size={42} onClick={() => t && api.togglePlay()} />
        <button onClick={() => api.next()} disabled={!t} className="rounded-full p-2 text-muted transition hover:text-fg disabled:opacity-40" title="Следующий">
          <SkipForward size={20} fill="currentColor" />
        </button>
        <button
          onClick={() => api.setRepeat(nextRepeat[player.repeat])}
          className={`relative hidden rounded-full p-2 transition sm:block ${player.repeat !== "off" ? "text-accent" : "text-muted hover:text-fg"}`}
          title={player.repeat === "off" ? "Повтор выключен" : player.repeat === "all" ? "Повтор очереди" : "Повтор трека"}
        >
          {player.repeat === "one" ? <Repeat1 size={18} /> : <Repeat size={18} />}
          {player.repeat !== "off" && <span className="absolute bottom-0.5 left-1/2 h-1 w-1 -translate-x-1/2 rounded-full bg-accent text-on-accent" />}
        </button>
      </div>

      {/* Waveform seek bar */}
      <div className="flex min-w-0 flex-1 items-center gap-2 md:gap-3">
        <span className="hidden w-12 text-right text-xs tabular-nums text-accent md:inline">{formatTime(player.position)}</span>
        <div className="min-w-0 flex-1">
          {t ? (
            <Waveform key={t.id} url={t.waveformUrl} seed={t.id} progress={progress} onSeek={seekFraction} height={44} step={3} />
          ) : (
            <div className="h-[44px]" />
          )}
        </div>
        <span className="hidden w-12 text-xs tabular-nums text-muted md:inline">{formatTime(player.duration)}</span>
      </div>

      {/* Volume + link */}
      <div className="flex shrink-0 items-center gap-2 lg:w-[190px]">
        <button onClick={toggleMute} className="rounded-full p-2 text-muted transition hover:text-fg" title="Звук">
          <VolIcon size={18} />
        </button>
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={volume}
          onChange={(e) => changeVolume(Number(e.target.value))}
          className="slider hidden w-full lg:block"
          style={{ ["--fill" as string]: `${volume * 100}%` }}
          aria-label="Громкость"
        />
        {t?.permalinkUrl && (
          <button onClick={() => api.openUrl(t.permalinkUrl!)} className="hidden rounded-full p-2 text-muted transition hover:text-fg xl:block" title="Открыть на SoundCloud">
            <ExternalLink size={16} />
          </button>
        )}
      </div>
    </footer>
  );
}
