import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText } from "./api";
import type { AuthStatus, PlayerState, Track } from "./types";

interface Toast {
  id: number;
  text: string;
  kind: "error" | "info";
}

interface AppCtx {
  auth: AuthStatus | null;
  setAuth: (a: AuthStatus) => void;
  refreshAuth: () => Promise<void>;
  player: PlayerState;
  likedIds: Set<number>;
  toggleLike: (track: Track) => Promise<void>;
  /** Play `tracks[index]`, or toggle if it is already the current track. */
  playFrom: (tracks: Track[], index: number, startFraction?: number) => void;
  seekFraction: (fraction: number) => void;
  toasts: Toast[];
  notify: (text: string, kind?: Toast["kind"]) => void;
}

const initialPlayer: PlayerState = {
  track: null,
  index: null,
  queueLen: 0,
  status: "idle",
  position: 0,
  duration: 0,
  volume: 0.8,
  repeat: "off",
  liked: false,
  preview: false,
  error: null,
  outputDevice: null,
};

const Ctx = createContext<AppCtx | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [player, setPlayer] = useState<PlayerState>(initialPlayer);
  const [likedIds, setLikedIds] = useState<Set<number>>(new Set());
  const [toasts, setToasts] = useState<Toast[]>([]);
  const lastError = useRef<string | null>(null);

  const notify = useCallback((text: string, kind: Toast["kind"] = "error") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t.slice(-3), { id, text, kind }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 5000);
  }, []);

  const refreshAuth = useCallback(async () => {
    try {
      setAuth(await api.authStatus());
    } catch (e) {
      notify(errorText(e));
    }
  }, [notify]);

  useEffect(() => {
    refreshAuth();
    api.playerState().then(setPlayer).catch(() => {});
    const un = listen<PlayerState>("player://state", (e) => setPlayer(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, [refreshAuth]);

  // Surface playback errors once.
  useEffect(() => {
    if (player.error && player.error !== lastError.current) notify(player.error);
    lastError.current = player.error;
  }, [player.error, notify]);

  // Load liked ids whenever the account changes.
  useEffect(() => {
    if (!auth || auth.mode === "anonymous") {
      setLikedIds(new Set());
      return;
    }
    api.likedIds().then((ids) => setLikedIds(new Set(ids))).catch(() => {});
  }, [auth?.mode, auth?.user?.id]);

  const toggleLike = useCallback(
    async (track: Track) => {
      if (!auth || auth.mode === "anonymous") {
        notify("Чтобы ставить лайки, войдите в аккаунт (Настройки)", "info");
        return;
      }
      const like = !likedIds.has(track.id);
      setLikedIds((s) => {
        const n = new Set(s);
        if (like) n.add(track.id);
        else n.delete(track.id);
        return n;
      });
      try {
        await api.setLike(track.id, like);
      } catch (e) {
        setLikedIds((s) => {
          const n = new Set(s);
          if (like) n.delete(track.id);
          else n.add(track.id);
          return n;
        });
        notify(errorText(e));
      }
    },
    [auth, likedIds, notify],
  );

  const playFrom = useCallback(
    (tracks: Track[], index: number, startFraction?: number) => {
      const t = tracks[index];
      if (!t) return;
      if (player.track?.id === t.id && startFraction === undefined) {
        api.togglePlay();
        return;
      }
      if (player.track?.id === t.id && startFraction !== undefined) {
        api.seek(startFraction * (t.durationMs / 1000));
        if (player.status === "paused") api.togglePlay();
        return;
      }
      // Optimistic UI while the backend resolves the stream.
      setPlayer((p) => ({ ...p, track: t, status: "loading", position: 0, duration: t.durationMs / 1000, error: null }));
      api.playTracks(tracks, index, startFraction).catch((e) => notify(errorText(e)));
    },
    [player.track?.id, player.status, notify],
  );

  const seekFraction = useCallback(
    (f: number) => {
      if (!player.track) return;
      const secs = f * player.duration;
      setPlayer((p) => ({ ...p, position: secs }));
      api.seek(secs);
    },
    [player.track, player.duration],
  );

  const value = useMemo<AppCtx>(
    () => ({ auth, setAuth, refreshAuth, player, likedIds, toggleLike, playFrom, seekFraction, toasts, notify }),
    [auth, refreshAuth, player, likedIds, toggleLike, playFrom, seekFraction, toasts, notify],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useApp(): AppCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useApp outside AppProvider");
  return c;
}
