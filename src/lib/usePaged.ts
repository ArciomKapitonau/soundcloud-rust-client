import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorText } from "./api";
import type { Page, Track } from "./types";

/** Load a paginated track list, with "load more" following `nextHref`. */
export function usePaged(loader: (() => Promise<Page>) | null, deps: unknown[]) {
  const [tracks, setTracks] = useState<Track[]>([]);
  const [next, setNext] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  const reload = useCallback(() => {
    if (!loader) return;
    const id = ++reqId.current;
    setLoading(true);
    setError(null);
    loader()
      .then((p) => {
        if (id !== reqId.current) return;
        setTracks(p.tracks);
        setNext(p.nextHref);
      })
      .catch((e) => id === reqId.current && setError(errorText(e)))
      .finally(() => id === reqId.current && setLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  useEffect(reload, [reload]);

  const loadMore = useCallback(() => {
    if (!next || loadingMore) return;
    const id = reqId.current;
    setLoadingMore(true);
    api
      .loadNext(next)
      .then((p) => {
        if (id !== reqId.current) return;
        setTracks((t) => {
          const seen = new Set(t.map((x) => x.id));
          return [...t, ...p.tracks.filter((x) => !seen.has(x.id))];
        });
        setNext(p.nextHref);
      })
      .catch((e) => setError(errorText(e)))
      .finally(() => setLoadingMore(false));
  }, [next, loadingMore]);

  return { tracks, loading, loadingMore, error, hasMore: !!next, loadMore, reload };
}
