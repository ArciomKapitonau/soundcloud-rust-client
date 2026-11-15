import { useEffect, useState } from "react";
import { api, errorText } from "../lib/api";
import type { User } from "../lib/types";
import { usePaged } from "../lib/usePaged";
import { TrackGrid } from "../components/TrackGrid";
import { Cover } from "../components/Cover";
import { ErrorBox, ViewHeader } from "../components/ViewHeader";

export function Artist({ id, back }: { id: number; back: () => void }) {
  const [user, setUser] = useState<User | null>(null);
  const [error, setError] = useState<string | null>(null);
  const tracks = usePaged(() => api.artistTracks(id), [id]);
  useEffect(() => {
    let active = true;
    api.artist(id).then((u) => { if (active) setUser(u); }).catch((e) => { if (active) setError(errorText(e)); });
    return () => { active = false; };
  }, [id]);
  return <div>
    <button onClick={back} className="mb-5 text-accent">← Назад</button>
    {error && <ErrorBox text={error} />}
    <div className="mb-6 flex items-center gap-5">
      <Cover src={user?.avatarUrl ?? null} alt={user?.username ?? "Исполнитель"} className="h-24 w-24" rounded="rounded-full" />
      <div><ViewHeader title={user?.username ?? "Исполнитель"} subtitle={user?.followersCount != null ? `${user.followersCount.toLocaleString()} подписчиков` : undefined} />
      {user?.permalinkUrl && <button className="text-sm text-accent" onClick={() => api.openUrl(user.permalinkUrl!)}>Открыть на SoundCloud ↗</button>}</div>
    </div>
    {tracks.error && <ErrorBox text={tracks.error} onRetry={tracks.reload} />}
    <TrackGrid {...tracks} onLoadMore={tracks.loadMore} />
  </div>;
}
