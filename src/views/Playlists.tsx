import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, ListMusic, Play } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useApp } from "../lib/store";
import { formatTime } from "../lib/format";
import type { Playlist, Track } from "../lib/types";
import { Cover } from "../components/Cover";
import { TrackGrid } from "../components/TrackGrid";
import { ErrorBox, ViewHeader } from "../components/ViewHeader";
import { NeedLogin } from "./NeedLogin";

export function Playlists({ goSettings, albums = false }: { goSettings: () => void; albums?: boolean }) {
  const { auth } = useApp();
  const anon = auth?.mode === "anonymous";
  const [lists, setLists] = useState<Playlist[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<Playlist | null>(null);

  const load = useCallback(() => {
    setError(null);
    setLists(null);
    api.playlists().then((items) => setLists(items.filter((p) => p.isAlbum === albums))).catch((e) => {
      setError(errorText(e));
      setLists([]);
    });
  }, [albums]);

  useEffect(() => {
    if (!anon) load();
  }, [anon, auth?.user?.id, load]);

  if (anon) return <NeedLogin icon={ListMusic} what={albums ? "альбомы" : "плейлисты"} goSettings={goSettings} />;
  if (open) return <PlaylistDetail playlist={open} back={() => setOpen(null)} />;

  return (
    <div>
      <ViewHeader title={albums ? "Альбомы" : "Плейлисты"} subtitle={albums ? "Ваши и понравившиеся альбомы" : "Ваши и понравившиеся плейлисты"} onRefresh={load} />
      {error && <ErrorBox text={error} onRetry={load} />}
      {lists === null ? (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(min(150px,45%),1fr))] gap-5">
          {Array.from({ length: 10 }, (_, i) => (
            <div key={i}>
              <div className="skeleton aspect-square rounded-lg" />
              <div className="skeleton mt-3 h-3 w-3/4 rounded-sm" />
            </div>
          ))}
        </div>
      ) : lists.length === 0 && !error ? (
        <div className="py-20 text-center text-muted">Релизов пока нет</div>
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(min(150px,45%),1fr))] gap-5">
          {lists.map((p) => (
            <button key={p.id} onClick={() => setOpen(p)} className="group rounded-lg p-2 text-left transition hover:bg-surface-2">
              <div className="relative aspect-square overflow-hidden rounded-md shadow-lg shadow-black/40">
                <Cover src={p.artworkUrl} alt={p.title} className="h-full w-full transition duration-300 group-hover:scale-105" />
                <div className="absolute bottom-2 right-2 rounded-sm bg-black/70 px-2 py-0.5 text-[11px] font-semibold">{p.trackCount} тр.</div>
              </div>
              <div className="mt-3 truncate text-sm font-semibold">{p.title}</div>
              <div className="truncate text-xs text-muted">{p.artist}</div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function PlaylistDetail({ playlist, back }: { playlist: Playlist; back: () => void }) {
  const { playFrom } = useApp();
  const [tracks, setTracks] = useState<Track[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    (playlist.systemId ? api.systemPlaylistTracks(playlist.systemId) : api.playlistTracks(playlist.id))
      .then(setTracks)
      .catch((e) => setError(errorText(e)))
      .finally(() => setLoading(false));
  }, [playlist.id, playlist.systemId]);
  useEffect(load, [load]);

  return (
    <div>
      <button onClick={back} className="mb-4 flex items-center gap-2 text-sm text-muted transition hover:text-fg">
        <ArrowLeft size={16} /> Назад
      </button>
      <div className="mb-8 flex flex-col gap-4 sm:flex-row sm:items-end sm:gap-6">
        <Cover src={playlist.artworkUrl} alt={playlist.title} className="h-36 w-36 shrink-0 shadow-2xl shadow-black/60 md:h-48 md:w-48" rounded="rounded-lg" />
        <div className="min-w-0">
          <div className="text-xs font-semibold uppercase tracking-wider text-accent">{playlist.isAlbum ? "Альбом" : "Плейлист"}</div>
          <h1 className="mt-1 truncate text-2xl font-bold tracking-tight md:text-4xl">{playlist.title}</h1>
          <div className="mt-2 text-sm text-muted">
            {playlist.artist} · {playlist.trackCount} треков · {formatTime(playlist.durationMs / 1000)}
          </div>
          <button
            disabled={!tracks.length}
            onClick={() => playFrom(tracks, 0)}
            className="mt-5 flex items-center gap-2 rounded-pill bg-accent px-6 py-2.5 text-sm font-semibold transition hover:bg-accent-hover disabled:opacity-50 text-on-accent"
          >
            <Play size={16} fill="currentColor" /> Слушать
          </button>
        </div>
      </div>
      {error && <ErrorBox text={error} onRetry={load} />}
      <TrackGrid tracks={tracks} loading={loading} />
    </div>
  );
}
