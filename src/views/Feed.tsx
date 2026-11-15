import { api } from "../lib/api";
import { useApp } from "../lib/store";
import { usePaged } from "../lib/usePaged";
import { TrackGrid } from "../components/TrackGrid";
import { ErrorBox, ViewHeader } from "../components/ViewHeader";

export function Feed() {
  const { auth, playFrom } = useApp();
  const anon = auth?.mode === "anonymous";
  const feed = usePaged(anon ? null : api.feed, [auth?.mode, auth?.user?.id]);


  return (
    <div>
      <ViewHeader
        title="Подписки"
        subtitle={anon ? "Войдите в аккаунт, чтобы видеть ленту подписок." : "Новое от исполнителей, на которых вы подписаны"}
        onRefresh={feed.reload}
        onPlayAll={feed.tracks.length ? () => playFrom(feed.tracks, 0) : undefined}
      />
      {feed.error && <ErrorBox text={feed.error} onRetry={feed.reload} />}
      <TrackGrid {...feed} />
    </div>
  );
}
