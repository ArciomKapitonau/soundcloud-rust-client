import { memo } from "react";
import { ExternalLink, Heart, Play as PlayIcon } from "lucide-react";
import type { Track } from "../lib/types";
import { formatCount, formatTime, timeAgo } from "../lib/format";
import { api } from "../lib/api";
import { Cover } from "./Cover";
import { PlayButton } from "./PlayButton";
import { Waveform } from "./Waveform";

interface Props {
  track: Track;
  isCurrent: boolean;
  status: "idle" | "loading" | "playing" | "paused";
  progress: number | null;
  liked: boolean;
  onPlay: () => void;
  onSeek: (fraction: number) => void;
  onLike: () => void;
}

function TrackCardImpl({ track, isCurrent, status, progress, liked, onPlay, onSeek, onLike }: Props) {
  const btnState = isCurrent ? (status === "playing" ? "playing" : status === "loading" ? "loading" : "paused") : "paused";
  return (
    <div
      className={`track-card themed-card group flex gap-3 rounded-lg p-2.5 sm:gap-4 sm:p-3 ${
        isCurrent ? "bg-surface-2 ring-1 ring-accent/40" : "bg-surface hover:bg-surface-2"
      }`}
    >
      <div className="relative h-[88px] w-[88px] shrink-0 overflow-hidden rounded-sm sm:h-[120px] sm:w-[120px]">
        <Cover src={track.artworkUrl} alt={track.title} className="h-full w-full" />
        <div
          className={`absolute inset-0 flex items-center justify-center bg-black/40 transition ${
            isCurrent ? "opacity-100" : "opacity-0 group-hover:opacity-100"
          }`}
        >
          <PlayButton
            state={btnState}
            onClick={(e) => {
              e.stopPropagation();
              onPlay();
            }}
          />
        </div>
      </div>

      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <div className="truncate text-[13px] text-muted">{track.artist}</div>
            <div className="truncate text-[15px] font-semibold text-fg" title={track.title}>
              {track.title}
            </div>
          </div>
          <div className="hidden shrink-0 flex-col items-end gap-1 sm:flex">
            <span className="text-xs text-dim">{timeAgo(track.createdAt)}</span>
            {track.genre && (
              <span className="max-w-[120px] truncate rounded-pill bg-surface-3 px-2 py-0.5 text-[11px] text-muted">
                #{track.genre}
              </span>
            )}
          </div>
        </div>

        <div className="mt-auto pt-2">
          <Waveform url={track.waveformUrl} seed={track.id} progress={isCurrent ? progress : null} onSeek={onSeek} height={52} />
        </div>

        <div className="mt-1.5 flex items-center gap-3 text-xs text-dim">
          <button
            onClick={onLike}
            className={`flex items-center gap-1 rounded-sm px-1.5 py-0.5 transition hover:bg-surface-3 ${liked ? "text-accent" : "hover:text-fg"}`}
            title={liked ? "Убрать лайк" : "Нравится"}
          >
            <Heart size={13} fill={liked ? "currentColor" : "none"} />
            {formatCount(track.likesCount)}
          </button>
          {track.playbackCount != null && (
            <span className="flex items-center gap-1">
              <PlayIcon size={12} />
              {formatCount(track.playbackCount)}
            </span>
          )}
          {track.permalinkUrl && (
            <button
              onClick={() => api.openUrl(track.permalinkUrl!)}
              className="rounded-sm p-0.5 opacity-0 transition hover:text-fg group-hover:opacity-100"
              title="Открыть на soundcloud.com"
            >
              <ExternalLink size={13} />
            </button>
          )}
          <span className="ml-auto tabular-nums">{formatTime(track.durationMs / 1000)}</span>
        </div>
      </div>
    </div>
  );
}

export const TrackCard = memo(TrackCardImpl);
