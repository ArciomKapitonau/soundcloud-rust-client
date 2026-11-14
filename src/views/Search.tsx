import { useEffect, useRef, useState } from "react";
import { Search as SearchIcon, X } from "lucide-react";
import { api } from "../lib/api";
import { usePaged } from "../lib/usePaged";
import { TrackGrid } from "../components/TrackGrid";
import { ErrorBox } from "../components/ViewHeader";

const suggestions = ["lofi", "techno", "drum and bass", "ambient", "phonk", "jazz", "house", "hip hop"];

export function Search({ query, setQuery }: { query: string; setQuery: (q: string) => void }) {
  const [text, setText] = useState(query);
  const input = useRef<HTMLInputElement>(null);
  const results = usePaged(query ? () => api.search(query) : null, [query]);

  useEffect(() => input.current?.focus(), []);
  // Debounced live search.
  useEffect(() => {
    const t = setTimeout(() => text.trim() !== query && setQuery(text.trim()), 450);
    return () => clearTimeout(t);
  }, [text, query, setQuery]);

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
            placeholder="Исполнители, треки, жанры…"
            className="flex-1 bg-transparent text-[15px] outline-none placeholder:text-dim"
          />
          {text && (
            <button type="button" onClick={() => { setText(""); setQuery(""); }} className="text-muted hover:text-fg">
              <X size={18} />
            </button>
          )}
        </div>
      </form>

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
          {results.error && <ErrorBox text={results.error} onRetry={results.reload} />}
          <TrackGrid {...results} />
        </>
      )}
    </div>
  );
}
