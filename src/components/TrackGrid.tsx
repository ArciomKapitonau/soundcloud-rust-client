import { useMemo } from "react";
import { Loader2 } from "lucide-react";
import type { Track } from "../lib/types";
import { useApp } from "../lib/store";
import { TrackCard } from "./TrackCard";

interface Props {
  tracks: Track[];
  loading?: boolean;
  hasMore?: boolean;
  loadingMore?: boolean;
  onLoadMore?: () => void;
  empty?: React.ReactNode;
}

export function TrackGrid({ tracks, loading, hasMore, loadingMore, onLoadMore, empty }: Props) {
  const { player, likedIds, playFrom, seekFraction, toggleLike, openArtist } = useApp();

  // Keep event props stable between position ticks so TrackCard.memo can skip
  // every card except the currently playing one.
  const currentId = player.track?.id;
  const actions = useMemo(() => tracks.map((track, index) => ({
    onPlay: () => playFrom(tracks, index),
    onSeek: (fraction: number) => currentId === track.id ? seekFraction(fraction) : playFrom(tracks, index, fraction),
    onLike: () => toggleLike(track),
  })), [tracks, currentId, playFrom, seekFraction, toggleLike]);

  if (loading) {
    return (
      <div className="grid grid-cols-[repeat(auto-fill,minmax(min(440px,100%),1fr))] gap-3">
        {Array.from({ length: 8 }, (_, i) => (
          <div key={i} className="flex gap-4 rounded-lg bg-surface p-3">
            <div className="skeleton h-[88px] w-[88px] rounded-sm sm:h-[120px] sm:w-[120px]" />
            <div className="flex flex-1 flex-col gap-2 pt-1">
              <div className="skeleton h-3 w-1/3 rounded-sm" />
              <div className="skeleton h-4 w-2/3 rounded-sm" />
              <div className="skeleton mt-auto h-12 w-full rounded-sm" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  if (!tracks.length) return <>{empty ?? <div className="py-20 text-center text-muted">Ничего не найдено</div>}</>;

  const progress = player.duration > 0 ? player.position / player.duration : 0;

  return (
    <>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(min(440px,100%),1fr))] gap-3">
        {tracks.map((t, i) => {
          const isCurrent = player.track?.id === t.id;
          return (
            <TrackCard
              key={`${t.id}-${i}`}
              track={t}
              openArtist={openArtist}
              isCurrent={isCurrent}
              status={isCurrent ? player.status : "idle"}
              progress={isCurrent ? progress : null}
              liked={likedIds.has(t.id)}
              {...actions[i]}
            />
          );
        })}
      </div>
      {hasMore && (
        <div className="flex justify-center py-6">
          <button
            onClick={onLoadMore}
            disabled={loadingMore}
            className="flex items-center gap-2 rounded-pill border border-line px-6 py-2 text-sm text-muted transition hover:border-accent hover:text-fg disabled:opacity-60"
          >
            {loadingMore && <Loader2 size={14} className="animate-spin" />}
            Загрузить ещё
          </button>
        </div>
      )}
    </>
  );
}
