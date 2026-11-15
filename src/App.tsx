import { useEffect, useRef, useState } from "react";
import { Loader2 } from "lucide-react";
import { useApp } from "./lib/store";
import { api } from "./lib/api";
import { Sidebar, type View } from "./components/Sidebar";
import { PlayerBar } from "./components/PlayerBar";
import { Toasts } from "./components/Toasts";
import { Artist } from "./views/Artist";
import { Feed } from "./views/Feed";
import { Home } from "./views/Home";
import { Search } from "./views/Search";
import { Likes } from "./views/Likes";
import { Playlists } from "./views/Playlists";
import { Settings } from "./views/Settings";
import { Login } from "./views/Login";

export default function App() {
  const { auth, player, artistId, openArtist } = useApp();
  const [view, setView] = useState<View>("home");
  const [query, setQuery] = useState("");
  const main = useRef<HTMLElement>(null);

  useEffect(() => main.current?.scrollTo({ top: 0 }), [view]);

  // Keyboard shortcuts: Space = play/pause, ←/→ = seek ±10 s.
  const posRef = useRef(player.position);
  posRef.current = player.position;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT") return;
      if (e.code === "Space") {
        e.preventDefault();
        api.togglePlay();
      } else if (e.code === "ArrowRight" && e.shiftKey) api.next();
      else if (e.code === "ArrowLeft" && e.shiftKey) api.prev();
      else if (e.code === "ArrowRight") api.seek(posRef.current + 10);
      else if (e.code === "ArrowLeft") api.seek(Math.max(0, posRef.current - 10));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!auth) {
    return (
      <div className="flex h-full items-center justify-center text-accent">
        <Loader2 className="animate-spin" size={32} />
      </div>
    );
  }
  if (!auth.onboarded) {
    return (
      <>
        <Login />
        <Toasts />
      </>
    );
  }

  const goSettings = () => setView("settings");
  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <Sidebar view={view} setView={(next) => { openArtist(null); setView(next); }} />
        <main ref={main} className="min-w-0 flex-1 overflow-y-auto px-4 py-5 md:px-8 md:py-7">
          {artistId !== null ? <Artist key={artistId} id={artistId} back={() => openArtist(null)} /> : <>
          {view === "feed" && <Feed />}
          {view === "albums" && <Playlists key="albums" albums goSettings={goSettings} />}
          {view === "home" && <Home />}
          {view === "search" && <Search query={query} setQuery={setQuery} />}
          {view === "likes" && <Likes goSettings={goSettings} />}
          {view === "playlists" && <Playlists key="playlists" goSettings={goSettings} />}
          {view === "settings" && <Settings />}
          </>}
        </main>
      </div>
      <PlayerBar />
      <Toasts />
    </div>
  );
}
