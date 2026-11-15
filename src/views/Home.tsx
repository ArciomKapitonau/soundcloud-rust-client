import { useEffect, useState } from "react";
import { api, errorText } from "../lib/api";
import { useApp } from "../lib/store";
import type { DiscoverySection, Playlist, Track } from "../lib/types";
import { Cover } from "../components/Cover";
import { ErrorBox, ViewHeader } from "../components/ViewHeader";
import { PlaylistDetail } from "./Playlists";

export function Home() {
  const { auth, playFrom } = useApp();
  const [sections, setSections] = useState<DiscoverySection[]>([]);
  const [likes, setLikes] = useState<Track[]>([]);
  const [open, setOpen] = useState<Playlist | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [revision, refresh] = useState(0);
  useEffect(() => {
    let active = true;
    setLoading(true); setError(null); setSections([]); setLikes([]);
    api.discover().then((rows) => { if (active) setSections(rows); })
      .catch((e) => { if (active) setError(errorText(e)); })
      .finally(() => { if (active) setLoading(false); });
    if (auth?.mode !== "anonymous") api.likes().then((p) => { if (active) setLikes(p.tracks.slice(0, 8)); }).catch(() => {});
    return () => { active = false; };
  }, [auth?.mode, auth?.user?.id, revision]);
  if (open) return <PlaylistDetail playlist={open} back={() => setOpen(null)} />;
  return (
    <div>
      <ViewHeader title="Главная" subtitle="Открывайте музыку и подборки SoundCloud" onRefresh={() => refresh((v) => v + 1)} />
      <div className="flex items-start gap-10">
        <div className="min-w-0 flex-1 space-y-10">
          {error && <ErrorBox text={error} onRetry={() => refresh((v) => v + 1)} />}
          {loading && <div className="flex gap-5 overflow-hidden" aria-label="Загрузка подборок">{Array.from({ length: 5 }, (_, i) => <div key={i} className="skeleton h-48 w-40 shrink-0 rounded-lg" />)}</div>}
          {!loading && !error && !sections.length && <p className="text-muted">SoundCloud пока не вернул доступных подборок. Музыку можно найти через поиск.</p>}
          {sections.map((section, index) => (
            <section key={`${section.title}-${index}`}>
              <h2 className="mb-4 text-xl font-bold">{section.title}</h2>
              <div className="flex snap-x gap-5 overflow-x-auto pb-4">
                {section.tracks.map((track, i) => (
                  <button key={track.id} onClick={() => playFrom(section.tracks, i)} className="w-40 shrink-0 snap-start text-left group">
                    <Cover src={track.artworkUrl} alt={track.title} className="aspect-square w-full" rounded="rounded-md" />
                    <div className="mt-3 truncate font-semibold group-hover:text-accent" title={track.title}>{track.title}</div>
                    <div className="truncate text-sm text-muted">{track.artist}</div>
                  </button>
                ))}
                {section.playlists.map((playlist) => (
                  <button key={playlist.systemId ?? playlist.id} onClick={() => setOpen(playlist)} className="w-40 shrink-0 snap-start text-left group">
                    <Cover src={playlist.artworkUrl} alt={playlist.title} className="aspect-square w-full" rounded="rounded-md" />
                    <div className="mt-3 truncate font-semibold group-hover:text-accent" title={playlist.title}>{playlist.title}</div>
                    <div className="truncate text-sm text-muted">{playlist.artist}</div>
                    <div className="text-xs text-dim">{playlist.isAlbum ? "Альбом" : "Плейлист"} · {playlist.trackCount} тр.</div>
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
        {likes.length > 0 && <aside className="hidden w-60 shrink-0 xl:block">
          <h2 className="mb-4 border-b border-line pb-2 text-sm font-semibold uppercase text-muted">Ваши лайки</h2>
          <div className="space-y-4">{likes.map((track, i) => <button key={track.id} onClick={() => playFrom(likes, i)} className="flex w-full gap-3 text-left">
            <Cover src={track.artworkUrl} alt={track.title} className="h-12 w-12 shrink-0" />
            <div className="min-w-0"><div className="truncate text-xs text-muted">{track.artist}</div><div className="truncate text-sm" title={track.title}>{track.title}</div></div>
          </button>)}</div>
        </aside>}
      </div>
    </div>
  );
}
