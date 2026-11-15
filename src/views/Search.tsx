import { useEffect, useRef, useState } from "react";
import { Search as SearchIcon, X } from "lucide-react";
import { useApp } from "../lib/store";
import type { Playlist, User } from "../lib/types";
import { Cover } from "../components/Cover";
import { PlaylistDetail } from "./Playlists";
import { api, errorText } from "../lib/api";
import { usePaged } from "../lib/usePaged";
import { TrackGrid } from "../components/TrackGrid";
import { ErrorBox } from "../components/ViewHeader";

const suggestions = ["lofi", "techno", "drum and bass", "ambient", "phonk", "jazz", "house", "hip hop"];

export function Search({ query, setQuery }: { query: string; setQuery: (q: string) => void }) {
  const { openArtist } = useApp();
  const [kind, setKind] = useState<"tracks" | "albums" | "users">("tracks");
  const [entries, setEntries] = useState<(Playlist | User)[]>([]);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [more, setMore] = useState(false);
  const [offset, setOffset] = useState(0);
  const [open, setOpen] = useState<Playlist | null>(null);
  const [revision, setRevision] = useState(0);
  const [text, setText] = useState(query);
  const input = useRef<HTMLInputElement>(null);
  const results = usePaged(query && kind === "tracks" ? () => api.search(query) : null, [query, kind]);

  useEffect(() => {
    let active = true;
    setEntries([]); setCatalogError(null); setMore(false);
    if (!query || kind === "tracks") { setBusy(false); return; }
    setBusy(true);
    api.searchCatalog<Playlist | User>(query, kind, offset).then((p) => {
      if (active) { setEntries(p.items); setMore(p.hasMore); }
    }).catch((e) => { if (active) setCatalogError(errorText(e)); })
      .finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, [query, kind, offset, revision]);
  useEffect(() => { setOffset(0); }, [query, kind]);
  useEffect(() => input.current?.focus(), []);
  // Debounced live search.
  useEffect(() => {
    const t = setTimeout(() => text.trim() !== query && setQuery(text.trim()), 450);
    return () => clearTimeout(t);
  }, [text, query, setQuery]);

  if (open) return <PlaylistDetail playlist={open} back={() => setOpen(null)} />;

  return (
    <div>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          setQuery(text.trim());
        }}
        className="mb-6"
      >
        <div className="flex items-center gap-3 rounded-pill border border-line bg-surface px-5 py-3 transition focus-within:border-accent">
          <SearchIcon size={20} className="text-muted" />
          <input
            ref={input}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Треки, альбомы, исполнители…"
            className="flex-1 bg-transparent text-[15px] outline-none placeholder:text-dim"
          />
          {text && (
            <button type="button" onClick={() => { setText(""); setQuery(""); }} className="text-muted hover:text-fg">
              <X size={18} />
            </button>
          )}
        </div>
      </form>

      <div className="mb-5 flex gap-2">{([ ["tracks", "Треки"], ["albums", "Альбомы"], ["users", "Исполнители"] ] as const).map(([key, label]) => <button key={key} onClick={() => { setKind(key); setOffset(0); }} className={`rounded-pill px-4 py-2 ${kind === key ? "bg-accent text-on-accent" : "bg-surface text-muted"}`}>{label}</button>)}</div>
      {!query ? (
        <div>
          <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-dim">Попробуйте</h2>
          <div className="flex flex-wrap gap-2">
            {suggestions.map((s) => (
              <button key={s} onClick={() => { setText(s); setQuery(s); }} className="rounded-pill bg-surface-2 px-4 py-2 text-sm text-muted transition hover:bg-accent-soft hover:text-accent">
                {s}
              </button>
            ))}
          </div>
        </div>
      ) : (
        <>
          <div className="mb-4 text-sm text-muted">
            Результаты по запросу <span className="font-semibold text-fg">«{query}»</span>
          </div>
          {kind === "tracks" ? <>
          {results.error && <ErrorBox text={results.error} onRetry={results.reload} />}
          <TrackGrid {...results} onLoadMore={results.loadMore} />
          </> : <>
            {catalogError && <ErrorBox text={catalogError} onRetry={() => setRevision((v) => v + 1)} />}
            {busy ? <p className="py-10 text-muted">Загрузка…</p> : <div className="grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-5">
              {entries.map((entry) => "username" in entry ? <button key={entry.id} onClick={() => openArtist(entry.id)} className="min-w-0 text-left">
                <Cover src={entry.avatarUrl} alt={entry.username} className="aspect-square w-full" rounded="rounded-full" />
                <div className="mt-3 truncate font-semibold">{entry.username}</div><div className="text-sm text-muted">{entry.followersCount?.toLocaleString() ?? "—"} подписчиков</div>
              </button> : <button key={entry.id} onClick={() => setOpen(entry)} className="min-w-0 text-left">
                <Cover src={entry.artworkUrl} alt={entry.title} className="aspect-square w-full" />
                <div className="mt-3 truncate font-semibold">{entry.title}</div><div className="truncate text-sm text-muted">{entry.artist} · {entry.trackCount} тр.</div>
              </button>)}
            </div>}
            {!busy && !catalogError && entries.length === 0 && <p className="py-10 text-muted">Ничего не найдено</p>}
            <div className="mt-6 flex gap-4">{offset > 0 && <button disabled={busy} onClick={() => setOffset((n) => Math.max(0, n - 30))}>← Предыдущая страница</button>}{more && <button disabled={busy} onClick={() => setOffset((n) => n + 30)}>Следующая страница →</button>}</div>
          </>}
        </>
      )}
    </div>
  );
}
