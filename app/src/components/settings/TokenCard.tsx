// 设置页第一张卡：现在有没有令牌、怎么拿、粘进去验一下。
import { useEffect, useRef, useState } from "react";
import ErrorHint from "../../ErrorHint";
import { call, toApiError, type ApiError, type TokenStatus } from "../../api";
import { t } from "../../i18n";

const ACCESS_PAGE = "https://mod.io/me/access";

export default function TokenCard() {
  const [status, setStatus] = useState<TokenStatus | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const started = useRef(false);

  async function refresh() {
    try {
      setStatus(await call<TokenStatus>("token_status"));
    } catch (e) {
      setError(toApiError(e));
    }
  }

  // 开发模式下 StrictMode 会把挂载流程跑两遍，这里挡一下，别让「这是谁」问两次。
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void refresh();
  }, []);

  async function save() {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      const res = await call<TokenStatus>("save_token", { token: draft });
      setStatus(res);
      setDraft("");
      setNote(res.note);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function clear() {
    setBusy(true);
    setError(null);
    try {
      setStatus(await call<TokenStatus>("clear_token"));
      setNote(t("settings.forgot"));
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className="card">
        <div className="card-title">{t("settings.status")}</div>
        <p className={status?.username ? "note ok" : "note"}>
          {status ? status.note : t("settings.statusLoading")}
          {status?.uid ? t("settings.uid", { uid: status.uid }) : ""}
        </p>
      </div>

      <div className="card">
        <div className="card-title">{t("settings.howTo")}</div>
        <ol className="steps">
          <li>{t("settings.step1", { url: ACCESS_PAGE })}</li>
          <li>{t("settings.step2")}</li>
          <li>{t("settings.step3")}</li>
        </ol>
        <p className="dim small">
          <a href={ACCESS_PAGE}>{ACCESS_PAGE}</a>
        </p>
        <p className="dim small">{t("settings.howToHint")}</p>
      </div>

      <div className="card">
        <div className="card-title">{t("settings.paste")}</div>
        <textarea
          className="input area"
          rows={3}
          placeholder={t("settings.pastePlaceholder")}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <div className="searchbar">
          <button className="btn" disabled={busy || draft.trim() === ""} onClick={() => void save()}>
            {busy ? t("action.savingToken") : t("action.saveToken")}
          </button>
          <button className="btn ghost" disabled={busy} onClick={() => void clear()}>
            {t("action.clearToken")}
          </button>
        </div>
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} onRetry={() => void refresh()} />}
    </>
  );
}
