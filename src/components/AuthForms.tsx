import { useState } from "react";
import { ExternalLink, KeyRound, Loader2, LogIn } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useApp } from "../lib/store";

export const inputCls =
  "w-full rounded-md border border-line bg-input px-3 py-2.5 text-sm outline-none transition placeholder:text-dim focus:border-accent";

export function Field({ label, hint, children }: { label: string; hint?: React.ReactNode; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-xs font-medium text-muted">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-[11px] leading-snug text-dim">{hint}</span>}
    </label>
  );
}

/** Official OAuth 2.1 + PKCE login (needs a registered SoundCloud app). */
export function OfficialLoginForm({ onDone }: { onDone?: () => void }) {
  const { auth, setAuth, notify } = useApp();
  const [clientId, setClientId] = useState(auth?.appClientId ?? "");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);

  const login = async () => {
    setBusy(true);
    try {
      const st = await api.loginOfficial(clientId, secret);
      setAuth(st);
      notify(st.user ? `Вы вошли как ${st.user.username}` : "Вход выполнен", "info");
      onDone?.();
    } catch (e) {
      notify(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      <p className="text-xs text-muted">Этот способ требует зарегистрированного API-приложения SoundCloud. Для его создания нужна подписка Artist Pro. Без реквизитов приложения используйте веб-сессию или гостевой режим.</p>
      <Field label="Client ID приложения">
        <input className={inputCls} value={clientId} onChange={(e) => setClientId(e.target.value)} placeholder="из soundcloud.com/you/apps" spellCheck={false} />
      </Field>
      <Field
        label="Client Secret"
        hint={auth?.hasAppSecret && !secret ? "Для нового OAuth-входа введите Client Secret приложения повторно." : "Redirect URI приложения: http://localhost:3000/callback"}
      >
        <input className={inputCls} type="password" value={secret} onChange={(e) => setSecret(e.target.value)} placeholder="••••••••" spellCheck={false} />
      </Field>
      <button
        onClick={login}
        disabled={busy || !clientId.trim()}
        className="flex w-full items-center justify-center gap-2 rounded-pill bg-accent py-3 text-on-accent text-sm font-semibold transition hover:bg-accent-hover disabled:opacity-50"
      >
        {busy ? <Loader2 size={16} className="animate-spin" /> : <LogIn size={16} />}
        {busy ? "Ожидаем вход в браузере…" : "Войти через SoundCloud OAuth"}
      </button>
      <button
        onClick={() => api.openUrl("https://soundcloud.com/you/apps")}
        className="flex items-center gap-1 text-[11px] text-dim transition hover:text-accent"
      >
        Зарегистрировать приложение <ExternalLink size={11} />
      </button>
    </div>
  );
}

/** Fallback: client_id / oauth_token copied from the web app (api-v2). */
export function WebTokenForm({ onDone }: { onDone?: () => void }) {
  const { auth, setAuth, notify } = useApp();
  const [clientId, setClientId] = useState(auth?.v2ClientId ?? "");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);

  const save = async () => {
    setBusy(true);
    try {
      const st = await api.setWebCredentials(clientId || null, token || null);
      setAuth(st);
      notify(st.user ? `Вы вошли как ${st.user.username}` : "Сохранено", "info");
      onDone?.();
    } catch (e) {
      notify(errorText(e));
      api.authStatus().then(setAuth).catch(() => {});
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      <p className="text-xs text-muted">Войдите на сайте SoundCloud в обычном браузере, в том числе через Google, затем перенесите oauth_token сюда. Регистрация API-приложения для этого режима не требуется. Браузерный вход сам по себе не подключает этот клиент.</p>
      <button type="button" onClick={() => api.openUrl("https://soundcloud.com/signin").catch((e) => notify(errorText(e)))} className="flex items-center gap-2 text-sm text-accent">
        <ExternalLink size={15} /> Открыть SoundCloud для входа
      </button>
      <Field label="client_id (необязательно)" hint="Пусто — клиент сам найдёт свежий публичный client_id в JS-бандлах soundcloud.com.">
        <input className={inputCls} value={clientId} onChange={(e) => setClientId(e.target.value)} placeholder="32 символа" spellCheck={false} />
      </Field>
      <Field
        label="oauth_token"
        hint={
          <>
            После входа: инструменты разработчика → Application / Storage → Cookies → soundcloud.com → <code className="text-muted">oauth_token</code>.
            {auth?.v2OauthToken && <> Сейчас сохранён: {auth.v2OauthToken}</>}
          </>
        }
      >
        <input className={inputCls} type="password" value={token} onChange={(e) => setToken(e.target.value)} placeholder="2-123456-…" spellCheck={false} />
      </Field>
      <button
        onClick={save}
        disabled={busy || (!clientId.trim() && !token.trim())}
        className="flex w-full items-center justify-center gap-2 rounded-pill border border-line py-2.5 text-sm font-semibold transition hover:border-accent hover:text-accent disabled:opacity-50"
      >
        {busy ? <Loader2 size={16} className="animate-spin" /> : <KeyRound size={16} />}
        Сохранить токены
      </button>
    </div>
  );
}
