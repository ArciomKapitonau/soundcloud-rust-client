export interface Track {
  id: number;
  title: string;
  artist: string;
  artistId: number | null;
  artistAvatar: string | null;
  artworkUrl: string | null;
  durationMs: number;
  waveformUrl: string | null;
  permalinkUrl: string | null;
  genre: string | null;
  playbackCount: number | null;
  likesCount: number | null;
  createdAt: string | null;
  media?: unknown;
  trackAuthorization?: string | null;
  policy?: string | null;
}

export interface Playlist {
  id: number;
  title: string;
  artist: string;
  artworkUrl: string | null;
  trackCount: number;
  durationMs: number;
  permalinkUrl: string | null;
}

export interface User {
  id: number;
  username: string;
  avatarUrl: string | null;
  permalinkUrl: string | null;
  followersCount: number | null;
}

export interface Page {
  tracks: Track[];
  nextHref: string | null;
}

export type Mode = "official" | "v2" | "anonymous";
export type RepeatMode = "off" | "all" | "one";
export type Status = "idle" | "loading" | "playing" | "paused";

export interface AuthStatus {
  mode: Mode;
  onboarded: boolean;
  user: User | null;
  appClientId: string | null;
  hasAppSecret: boolean;
  v2ClientId: string | null;
  v2OauthToken: string | null;
  scrapedClientId: string | null;
  officialExpiresAt: number | null;
  userError: string | null;
}

export interface PlayerState {
  track: Track | null;
  index: number | null;
  queueLen: number;
  status: Status;
  position: number;
  duration: number;
  volume: number;
  repeat: RepeatMode;
  liked: boolean;
  preview: boolean;
  error: string | null;
  outputDevice: string | null;
}

export interface Settings {
  volume: number;
  output_device: string | null;
  repeat: RepeatMode;
  close_to_tray: boolean;
}

export interface ThemeConfig {
  preset: string;
  customCss: string;
}
