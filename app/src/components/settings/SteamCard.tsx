// 设置页的 Steam 卡：粘 Web API key、看认出来的 SteamID。
//
// key 只存在本机（DPAPI 封装，`<数据目录>\steam-key.sealed`），不进 settings.json。
// 这里也**不去校验 key**——校验要打一次接口，设置页明确不做；会不会被接受，等好友页说话。
import { useCallback, useEffect, useState } from "react";
import ErrorHint from "../../ErrorHint";
import { call, toApiError, type ApiError, type SteamStatus } from "../../api";
import { t } from "../../i18n";
import { useAppStore } from "../../store/appStore";

export default function SteamCard() {
  const [status, setStatus] = useState<SteamStatus | null>(null);
  const [keyDraft, setKeyDraft] = useState("");
  const [idDraft, setIdDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const { page } = useAppStore();

  const refresh = useCallback(async () => {
    try {
      const s = await call<SteamStatus>("steam_status");
      setStatus(s);
      setIdDraft(s.steamId);
      setError(null);
    } catch (e) {
      setError(toApiError(e));
    }
  }, []);

  useEffect(() => {
    if (page !== "settings") return;
    void refresh();
  }, [page, refresh]);

  async function save() {
    setBusy(true);
    setError(null);
    setSaved(false);
    try {
      const s = await call<SteamStatus>("set_steam", {
        key: keyDraft.trim() || null,
        id: idDraft.trim() || null,
      });
      setStatus(s);
      setKeyDraft("");
      setSaved(true);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function forget() {
    setBusy(true);
    setError(null);
    setSaved(false);
    try {
      const s = await call<SteamStatus>("set_steam", { key: "", id: null });
      setStatus(s);
      setKeyDraft("");
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  const sourceText =
    status?.source === "client"
      ? t("steam.sourceClient")
      : status?.source === "manual"
        ? t("steam.sourceManual")
        : t("steam.sourceNone");

  return (
    <>
      <div className="card">
        <div className="card-title">{t("steam.title")}</div>
        <p className="dim small">{t("steam.hint")}</p>

        <p className="small">
          {status?.hasKey ? t("steam.keyStored") : t("steam.keyMissing")}
        </p>
        <div className="searchbar">
          <input
            className="input mono"
            value={keyDraft}
            placeholder={t("steam.keyPlaceholder")}
            spellCheck={false}
            aria-label={t("steam.keyLabel")}
            onChange={(e) => {
              setKeyDraft(e.target.value);
              setSaved(false);
            }}
          />
          <button className="btn" disabled={busy || keyDraft.trim() === ""} onClick={() => void save()}>
            {t("steam.save")}
          </button>
          {status?.hasKey && (
            <button className="btn ghost" disabled={busy} onClick={() => void forget()}>
              {t("steam.forget")}
            </button>
          )}
        </div>

        <p className="small" style={{ marginTop: 12 }}>
          {t("steam.idLabel")}{" "}
          <span className="mono">{status?.steamId || t("steam.idNone")}</span>{" "}
          <span className="dim">{sourceText}</span>
        </p>
        <div className="searchbar">
          <input
            className="input mono"
            value={idDraft}
            placeholder="7656119…"
            spellCheck={false}
            aria-label={t("steam.idLabel")}
            onChange={(e) => {
              setIdDraft(e.target.value);
              setSaved(false);
            }}
          />
          <button
            className="btn ghost"
            disabled={busy || idDraft.trim() === "" || idDraft.trim() === status?.steamId}
            onClick={() => void save()}
          >
            {t("steam.saveId")}
          </button>
        </div>
      </div>
      {saved && <p className="note ok">{t("steam.saved")}</p>}
      {error && <ErrorHint error={error} />}
    </>
  );
}
