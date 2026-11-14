import { api } from "../lib/api";
import { useApp } from "../lib/store";
import { usePaged } from "../lib/usePaged";
import { TrackGrid } from "../components/TrackGrid";
import { ErrorBox, ViewHeader } from "../components/ViewHeader";

export function Home() {
  const { auth, playFrom } = useApp();
  const anon = auth?.mode === "anonymous";
  const feed = usePaged(api.feed, [auth?.mode, auth?.user?.id]);
  const hour = new Date().getHours();
  const greet = hour < 6 ? "Доброй ночи" : hour < 12 ? "Доброе утро" : hour < 18 ? "Добрый день" : "Добрый вечер";

  return (
    <div>
      <ViewHeader
        title={anon ? "В тренде" : `${greet}${auth?.user ? `, ${auth.user.username}` : ""}`}
        subtitle={anon ? "Популярные треки SoundCloud. Войдите, чтобы видеть свою ленту." : "Новое от исполнителей, на которых вы подписаны"}
        onRefresh={feed.reload}
        onPlayAll={feed.tracks.length ? () => playFrom(feed.tracks, 0) : undefined}
      />
      {feed.error && <ErrorBox text={feed.error} onRetry={feed.reload} />}
      <TrackGrid {...feed} />
    </div>
  );
}
