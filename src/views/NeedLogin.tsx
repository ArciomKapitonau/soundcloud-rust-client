import type { Heart } from "lucide-react";

export function NeedLogin({ icon: Icon, what, goSettings }: { icon: typeof Heart; what: string; goSettings: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center py-28 text-center">
      <div className="mb-5 flex h-20 w-20 items-center justify-center rounded-full bg-accent-soft text-accent">
        <Icon size={36} />
      </div>
      <h2 className="text-xl font-semibold">Войдите, чтобы видеть {what}</h2>
      <p className="mt-2 max-w-md text-sm text-muted">
        В гостевом режиме доступны поиск и прослушивание. Для медиатеки войдите через SoundCloud или вставьте oauth_token из веб-версии.
      </p>
      <button onClick={goSettings} className="mt-6 rounded-pill bg-accent px-6 py-2.5 text-sm font-semibold transition hover:bg-accent-hover text-on-accent">
        Открыть настройки входа
      </button>
    </div>
  );
}
