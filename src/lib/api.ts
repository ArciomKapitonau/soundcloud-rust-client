import { invoke } from "@tauri-apps/api/core";
import type { User, DiscoverySection, AuthStatus, Page, PlayerState, Playlist, RepeatMode, Settings, ThemeConfig, Track } from "./types";

/** Typed wrappers around the Rust commands in `src-tauri/src/commands.rs`. */
export const api = {
  authStatus: () => invoke<AuthStatus>("auth_status"),
  loginOfficial: (clientId: string, clientSecret: string) =>
    invoke<AuthStatus>("login_official", { clientId, clientSecret }),
  setWebCredentials: (clientId: string | null, oauthToken: string | null) =>
    invoke<AuthStatus>("set_web_credentials", { clientId, oauthToken }),
  continueAnonymous: () => invoke<AuthStatus>("continue_anonymous"),
  refreshClientId: () => invoke<string>("refresh_client_id"),
  logout: () => invoke<AuthStatus>("logout"),

  systemPlaylistTracks: (systemId: string) => invoke<Track[]>("get_system_playlist_tracks", { systemId }),
  discover: () => invoke<DiscoverySection[]>("get_discover"),
  searchCatalog: <T>(query: string, kind: "albums" | "users", offset = 0) => invoke<{items: T[]; hasMore: boolean}>("search_catalog", {query, kind, offset}),
  artist: (artistId: number) => invoke<User>("get_artist", {artistId}),
  artistTracks: (artistId: number) => invoke<Page>("get_artist_tracks", {artistId}),
  feed: () => invoke<Page>("get_feed"),
  search: (query: string) => invoke<Page>("search_tracks", { query }),
  likes: () => invoke<Page>("get_likes"),
  loadNext: (nextHref: string) => invoke<Page>("load_next", { nextHref }),
  playlists: () => invoke<Playlist[]>("get_playlists"),
  playlistTracks: (playlistId: number) => invoke<Track[]>("get_playlist_tracks", { playlistId }),
  likedIds: () => invoke<number[]>("get_liked_ids"),
  setLike: (trackId: number, like: boolean) => invoke<boolean>("set_like", { trackId, like }),
  waveform: (url: string, bars = 200) => invoke<number[]>("get_waveform", { url, bars }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),

  playTracks: (tracks: Track[], index: number, startFraction?: number) =>
    invoke<void>("play_tracks", { tracks, index, startFraction: startFraction ?? null }),
  togglePlay: () => invoke<void>("toggle_play"),
  next: () => invoke<void>("next_track"),
  prev: () => invoke<void>("prev_track"),
  jumpTo: (index: number) => invoke<void>("jump_to", { index }),
  seek: (seconds: number) => invoke<void>("seek", { seconds }),
  setVolume: (volume: number) => invoke<void>("set_volume", { volume }),
  setRepeat: (mode: RepeatMode) => invoke<void>("set_repeat", { mode }),
  playerState: () => invoke<PlayerState>("player_state"),
  queue: () => invoke<Track[]>("get_queue"),
  audioDevices: () => invoke<string[]>("list_audio_devices"),
  setAudioDevice: (name: string | null) => invoke<void>("set_audio_device", { name }),
  settings: () => invoke<Settings>("get_settings"),
  setCloseToTray: (enabled: boolean) => invoke<void>("set_close_to_tray", { enabled }),

  getTheme: () => invoke<ThemeConfig>("get_theme"),
  saveTheme: (preset: string, customCss: string) => invoke<ThemeConfig>("save_theme", { preset, customCss }),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
