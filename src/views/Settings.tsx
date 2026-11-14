import { useEffect, useState, type ReactNode } from "react";
import { LogOut, RefreshCw, Speaker } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useApp } from "../lib/store";
import { Cover } from "../components/Cover";
import { OfficialLoginForm, WebTokenForm, inputCls } from "../components/AuthForms";
import { ViewHeader } from "../components/ViewHeader";
import { AppearanceSettings } from "../components/AppearanceSettings";

function Section({ title, desc, children }: { title: string; desc?: string; children: ReactNode }) {
  return (
    <section className="themed-card rounded-xl border-line bg-surface p-6">
      <h2 className="text-base font-semibold">{title}</h2>
      {desc && <p className="mt-1 text-sm text-muted">{desc}</p>}
      <div className="mt-5">{children}</div>
    </section>
  );
}

const modeLabel = { official: "Официальный OAuth 2.1 (api.soundcloud.com)", v2: "Веб-токен (api-v2.soundcloud.com)", anonymous: "Гостевой режим (публичный client_id)" };

export function Settings() {
  const { auth, setAuth, player, notify } = useApp();
  const [devices, setDevices] = useState<string[]>([]);
  const [closeToTray, setCloseToTray] = useState(true);
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    api.audioDevices().then(setDevices).catch(() => {});
    api.settings().then((s) => setCloseToTray(s.close_to_tray)).catch(() => {});
  }, []);

  const rescrape = async () => {
    setRefreshing(true);
    try {
      const id = await api.refreshClientId();
      notify(`Получен новый client_id: ${id}`, "info");
      setAuth(await api.authStatus());
    } catch (e) {
      notify(errorText(e));
    } finally {
      setRefreshing(false);
    }
  };

  return (
    <div className="max-w-3xl">
      <ViewHeader title="Настройки" />
      <div className="space-y-5">
        <Section title="Аккаунт">
          <div className="flex flex-wrap items-center gap-4">
            {auth?.user ? (
              <Cover src={auth.user.avatarUrl} alt={auth.user.username} className="h-14 w-14" rounded="rounded-full" />
            ) : (
              <div className="h-14 w-14 rounded-full bg-surface-3" />
            )}
            <div className="min-w-0 flex-1">
              <div className="font-semibold">{auth?.user?.username ?? "Не выполнен вход"}</div>
              <div className="text-sm text-muted">{auth ? modeLabel[auth.mode] : "…"}</div>
              {auth?.officialExpiresAt && (
                <div className="text-xs text-dim">Токен действует до {new Date(auth.officialExpiresAt * 1000).toLocaleString()} (обновляется автоматически)</div>
              )}
              {auth?.userError && <div className="mt-1 text-xs text-red-300">{auth.userError}</div>}
            </div>
            {auth?.mode !== "anonymous" && (
              <button
                onClick={async () => setAuth(await api.logout())}
                className="flex items-center gap-2 rounded-pill border border-line px-4 py-2 text-sm text-muted transition hover:border-red-400 hover:text-red-300"
              >
                <LogOut size={15} /> Выйти
              </button>
            )}
          </div>
        </Section>

        <div className="grid gap-5 md:grid-cols-2">
          <Section title="Вход через OAuth" desc="Официальный API: нужен client_id и secret вашего приложения SoundCloud.">
            <OfficialLoginForm />
          </Section>
          <Section title="Веб-токены" desc="Резервный режим для api-v2: client_id и/или oauth_token из веб-версии.">
            <WebTokenForm />
          </Section>
        </div>

        <Section title="Внешний вид" desc="Пресеты тем и собственный CSS. Изменения применяются мгновенно и сохраняются в ~/.config/sc-rust-client/theme.json.">
          <AppearanceSettings />
        </Section>

        <Section title="Публичный client_id" desc="Используется для api-v2, если вы не указали свой. Извлекается из JS-бандлов soundcloud.com.">
          <div className="flex flex-wrap items-center gap-3">
            <code className="min-w-0 flex-1 truncate rounded-md bg-input px-3 py-2.5 text-sm text-muted">{auth?.v2ClientId ?? auth?.scrapedClientId ?? "ещё не получен"}</code>
            <button
              onClick={rescrape}
              disabled={refreshing}
              className="flex items-center gap-2 rounded-pill border border-line px-4 py-2 text-sm text-muted transition hover:border-accent hover:text-fg disabled:opacity-50"
            >
              <RefreshCw size={15} className={refreshing ? "animate-spin" : ""} /> Обновить
            </button>
          </div>
        </Section>

        <Section title="Аудио">
          <label className="block">
            <span className="mb-1.5 flex items-center gap-2 text-xs font-medium text-muted">
              <Speaker size={14} /> Устройство вывода
            </span>
            <select
              className={inputCls}
              value={player.outputDevice ?? ""}
              onChange={(e) => api.setAudioDevice(e.target.value || null)}
              onFocus={() => api.audioDevices().then(setDevices).catch(() => {})}
            >
              <option value="">Системное по умолчанию</option>
              {devices.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </label>
          <label className="mt-5 flex cursor-pointer items-center justify-between gap-4">
            <div>
              <div className="text-sm font-medium">Сворачивать в трей при закрытии</div>
              <div className="text-xs text-dim">Музыка продолжит играть, окно можно вернуть из системного трея</div>
            </div>
            <button
              role="switch"
              aria-checked={closeToTray}
              onClick={() => {
                const v = !closeToTray;
                setCloseToTray(v);
                api.setCloseToTray(v);
              }}
              className={`relative h-6 w-11 shrink-0 rounded-full transition ${closeToTray ? "bg-accent" : "bg-surface-3"}`}
            >
              <span className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all ${closeToTray ? "left-[22px]" : "left-0.5"}`} />
            </button>
          </label>
        </Section>

        <p className="pb-4 text-center text-xs text-dim">Настройки: ~/.config/sc-rust-client/ (auth.json, settings.json, theme.json)</p>
      </div>
    </div>
  );
}
