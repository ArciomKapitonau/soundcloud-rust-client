import { Disc3, Radio, Heart, Home, ListMusic, Search, Settings } from "lucide-react";
import { useApp } from "../lib/store";
import { Cover } from "./Cover";

export type View = "home" | "feed" | "albums" | "search" | "likes" | "playlists" | "settings";

const Item = ({ active, icon: Icon, label, onClick }: { active: boolean; icon: typeof Home; label: string; onClick: () => void }) => (
  <button
    onClick={onClick}
    title={label}
    className={`flex w-full items-center justify-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition lg:justify-start ${
      active ? "bg-accent-soft text-accent" : "text-muted hover:bg-surface-2 hover:text-fg"
    }`}
  >
    <Icon size={18} className="shrink-0" />
    <span className="hidden truncate lg:inline">{label}</span>
  </button>
);

export function Sidebar({ view, setView }: { view: View; setView: (v: View) => void }) {
  const { auth, player } = useApp();
  const user = auth?.user;
  return (
    <aside className="flex w-[64px] shrink-0 flex-col overflow-y-auto overflow-x-hidden glass border-r border-line bg-sidebar lg:w-[232px]">
      <div className="flex items-center justify-center gap-2 px-3 pb-6 pt-5 lg:justify-start lg:px-5">
        <Logo />
        <span className="hidden text-[17px] font-bold tracking-tight lg:inline">SoundCloud</span>
      </div>

      <nav className="flex flex-col gap-1 px-2 lg:px-3">
        <Item active={view === "home"} icon={Home} label="Главная" onClick={() => setView("home")} />
        <Item active={view === "feed"} icon={Radio} label="Подписки" onClick={() => setView("feed")} />
        <Item active={view === "search"} icon={Search} label="Поиск" onClick={() => setView("search")} />
      </nav>

      <div className="mt-6 hidden px-6 pb-2 text-[11px] font-semibold uppercase tracking-wider text-dim lg:block">Моя медиатека</div>
      <div className="mx-4 my-3 h-px bg-line lg:hidden" />
      <nav className="flex flex-col gap-1 px-2 lg:px-3">
        <Item active={view === "albums"} icon={Disc3} label="Альбомы" onClick={() => setView("albums")} />
        <Item active={view === "likes"} icon={Heart} label="Лайки" onClick={() => setView("likes")} />
        <Item active={view === "playlists"} icon={ListMusic} label="Плейлисты" onClick={() => setView("playlists")} />
      </nav>

      <div className="mt-auto flex flex-col gap-1 px-2 pb-3 lg:px-3">
        {player.track && player.status === "playing" && (
          <div className="mb-2 flex items-center justify-center gap-2 rounded-md bg-surface px-3 py-2 text-xs text-muted lg:justify-start">
            <div className="flex h-3 items-end gap-[2px]">
              {[0, 0.2, 0.4].map((d) => (
                <span key={d} className="eq-bar block h-3 w-[3px] rounded-sm bg-accent text-on-accent" style={{ animationDelay: `${d}s` }} />
              ))}
            </div>
            <span className="hidden truncate lg:inline">Сейчас играет</span>
          </div>
        )}
        <Item active={view === "settings"} icon={Settings} label="Настройки" onClick={() => setView("settings")} />
        <div className="mt-2 flex items-center justify-center gap-3 rounded-md px-1 py-2 lg:justify-start lg:px-3">
          {user ? (
            <>
              <Cover src={user.avatarUrl} alt={user.username} className="h-8 w-8" rounded="rounded-full" />
              <div className="hidden min-w-0 lg:block">
                <div className="truncate text-sm font-medium">{user.username}</div>
                <div className="text-[11px] text-dim">{auth?.mode === "official" ? "OAuth" : "Веб-токен"}</div>
              </div>
            </>
          ) : (
            <div className="hidden text-xs text-dim lg:block">Гостевой режим</div>
          )}
        </div>
      </div>
    </aside>
  );
}

export function Logo({ size = 30 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 512 512" fillRule="evenodd" clipRule="evenodd" aria-hidden>
      {/* Same artwork as app-icon.svg; colours follow the active theme. */}
      <path style={{ fill: "var(--accent)" }} d="M105 0h302c57.928.155 104.845 47.072 105 104.996V407c-.155 57.926-47.072 104.844-104.996 104.998L105 512C47.074 511.844.156 464.926.002 407.003L0 105C.156 47.072 47.074.155 104.997 0H105z" />
      <path style={{ fill: "var(--on-accent)" }} d="M418.793 285.594c-1.365 24.398-21.728 43.377-46.159 43.039H261.682c-5.081-.053-9.183-4.176-9.211-9.252V199.917a10.18 10.18 0 016.128-9.727s10.206-7.078 31.701-7.078a72.716 72.716 0 0137.306 10.251 73.816 73.816 0 0134.735 48.107 42.846 42.846 0 0111.768-1.603 44.429 44.429 0 0132.052 13.379 44.413 44.413 0 0112.637 32.35l-.005-.002zM238.624 207.47c3.346 40.52 5.778 77.475 0 117.862a3.601 3.601 0 01-3.579 3.208 3.607 3.607 0 01-3.584-3.208c-5.387-40.044-3.04-77.693 0-117.862a3.59 3.59 0 011.671-3.446 3.625 3.625 0 013.829 0 3.599 3.599 0 011.663 3.446zm-22.45 117.902a3.772 3.772 0 01-3.732 3.208 3.77 3.77 0 01-3.736-3.208 450.489 450.489 0 010-104 3.781 3.781 0 013.757-3.378 3.785 3.785 0 013.756 3.378 404.554 404.554 0 01-.047 104h.002zm-22.497-107.518c3.644 37.128 5.302 70.392-.04 107.433a3.608 3.608 0 01-3.604 3.603 3.608 3.608 0 01-3.604-3.603c-5.169-36.565-3.434-70.783 0-107.433a3.633 3.633 0 013.624-3.246 3.636 3.636 0 013.624 3.246zm-22.53 107.565a3.7 3.7 0 01-3.677 3.286 3.683 3.683 0 01-3.664-3.286 380.496 380.496 0 010-97.102 3.735 3.735 0 017.468 0 354.847 354.847 0 01-.132 97.102h.005zm-22.502-72.824c5.692 25.18 3.127 47.419-.213 73.082a3.523 3.523 0 01-3.479 2.968 3.52 3.52 0 01-3.471-2.968c-3.041-25.312-5.56-48.075-.218-73.089a3.696 3.696 0 013.691-3.692 3.692 3.692 0 013.692 3.697l-.002.002zm-22.45-3.824c5.214 25.796 3.519 47.637-.133 73.52-.431 3.824-6.985 3.865-7.335 0-3.301-25.493-4.864-47.987-.133-73.52a3.833 3.833 0 013.804-3.414 3.813 3.813 0 013.797 3.414zm-22.663 12.504c5.467 17.111 3.599 31.013-.218 48.551a3.559 3.559 0 01-3.539 3.169 3.572 3.572 0 01-3.546-3.169c-3.301-17.195-4.638-31.395-.298-48.551a3.811 3.811 0 013.797-3.413 3.813 3.813 0 013.797 3.413h.007z" />
    </svg>
  );
}
