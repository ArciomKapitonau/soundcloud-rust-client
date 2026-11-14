import { useState } from "react";
import { ChevronDown, Loader2 } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useApp } from "../lib/store";
import { Logo } from "../components/Sidebar";
import { OfficialLoginForm, WebTokenForm } from "../components/AuthForms";

export function Login() {
  const { setAuth, notify } = useApp();
  const [tab, setTab] = useState<"oauth" | "web">("oauth");
  const [busy, setBusy] = useState(false);

  const guest = async () => {
    setBusy(true);
    try {
      setAuth(await api.continueAnonymous());
    } catch (e) {
      notify(`Не удалось получить client_id: ${errorText(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="relative flex h-full items-center justify-center overflow-auto p-6">
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(ellipse_at_top,var(--accent-soft),transparent_60%)]" />
      <div className="relative w-full max-w-[440px] themed-card glass rounded-2xl border-line bg-surface p-8 shadow-2xl shadow-black/60">
        <div className="mb-6 flex flex-col items-center text-center">
          <Logo size={56} />
          <h1 className="mt-4 text-2xl font-bold tracking-tight">SoundCloud для Linux</h1>
          <p className="mt-1 text-sm text-muted">Войдите, чтобы слушать ленту, лайки и плейлисты</p>
        </div>

        <div className="mb-5 grid grid-cols-2 rounded-full bg-input p-1 text-sm">
          {(["oauth", "web"] as const).map((t) => (
            <button
              key={t}
              onClick={() => setTab(t)}
              className={`rounded-pill py-2 font-medium transition ${tab === t ? "bg-surface-3 text-fg" : "text-muted hover:text-fg"}`}
            >
              {t === "oauth" ? "OAuth" : "Веб-токены"}
            </button>
          ))}
        </div>

        {tab === "oauth" ? <OfficialLoginForm /> : <WebTokenForm />}

        <div className="my-6 flex items-center gap-3 text-xs text-dim">
          <div className="h-px flex-1 bg-line" /> или <div className="h-px flex-1 bg-line" />
        </div>

        <button
          onClick={guest}
          disabled={busy}
          className="flex w-full items-center justify-center gap-2 rounded-pill py-2.5 text-sm text-muted transition hover:bg-surface-2 hover:text-fg"
        >
          {busy ? <Loader2 size={16} className="animate-spin" /> : <ChevronDown size={16} className="-rotate-90" />}
          Продолжить без входа
        </button>
        <p className="mt-2 text-center text-[11px] text-dim">Поиск и прослушивание через публичный client_id</p>
      </div>
    </div>
  );
}
