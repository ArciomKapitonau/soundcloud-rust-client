import { Heart } from "lucide-react";
import { api } from "../lib/api";
import { useApp } from "../lib/store";
import { usePaged } from "../lib/usePaged";
import { TrackGrid } from "../components/TrackGrid";
import { ErrorBox, ViewHeader } from "../components/ViewHeader";
import { NeedLogin } from "./NeedLogin";

export function Likes({ goSettings }: { goSettings: () => void }) {
  const { auth, playFrom } = useApp();
  const anon = auth?.mode === "anonymous";
  const likes = usePaged(anon ? null : api.likes, [auth?.mode, auth?.user?.id]);
  if (anon) return <NeedLogin icon={Heart} what="лайки" goSettings={goSettings} />;
  return (
    <div>
      <ViewHeader
        title="Лайки"
        subtitle="Треки, которые вам понравились"
        onRefresh={likes.reload}
        onPlayAll={likes.tracks.length ? () => playFrom(likes.tracks, 0) : undefined}
      />
      {likes.error && <ErrorBox text={likes.error} onRetry={likes.reload} />}
      <TrackGrid {...likes} empty={<div className="py-20 text-center text-muted">Здесь появятся треки, которые вы лайкнете</div>} />
    </div>
  );
}
