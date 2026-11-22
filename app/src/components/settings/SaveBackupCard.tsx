// 设置页的存档备份卡：备份一次 + 从备份还原。
//
// 还原的三步顺序是硬要求（决策 #29）：列备份（这张卡）→ 点还原 → 覆盖前先把当前档
// 再存一份。顺序反过来就等于「拿旧档盖掉今天的档、还顺手把今天的丢了」。
// 后端 restore_backup 先做安全备份、核对字节数，成了才覆盖；界面这里只负责把
// 「接下来要发生什么」写清楚，包括那句「旧档能不能被当前版本的游戏读，没有验证过」。
import { useCallback, useEffect, useState } from "react";
import ErrorHint from "../../ErrorHint";
import {
  call,
  toApiError,
  type ApiError,
  type RestoredSave,
  type SaveBackup,
  type SaveBackupList,
} from "../../api";
import { t, whenText } from "../../i18n";
import { useAppStore } from "../../store/appStore";

/** 把字节数说成人话（存档一般 1–3 MB）。 */
function sizeText(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

export default function SaveBackupCard() {
  const [info, setInfo] = useState<SaveBackupList | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [confirm, setConfirm] = useState<string | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const { page } = useAppStore();

  const refresh = useCallback(async () => {
    try {
      setInfo(await call<SaveBackupList>("save_backup_info"));
    } catch (e) {
      setError(toApiError(e));
    }
  }, []);

  // 页面一直挂着（切页只是 hidden），所以每次进设置页都重新读一遍。
  useEffect(() => {
    if (page !== "settings") return;
    void refresh();
  }, [page, refresh]);

  async function restore(name: string) {
    setBusy(true);
    setError(null);
    setNote("");
    setConfirm(null);
    try {
      const done = await call<RestoredSave>("restore_backup", { name });
      setNote(
        t("savebak.restored", { from: done.from, safety: done.safety.name }),
      );
      await refresh();
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function backupOnce() {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      const made = await call<SaveBackup>("backup_save");
      setNote(t("savebak.done", { name: made.name, size: sizeText(made.bytes) }));
      await refresh();
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className="card">
        <div className="card-title">{t("savebak.title")}</div>
        <p className="dim small">{t("savebak.hint")}</p>
        <p className="dim small" data-savebak-note="1">
          {t("savebak.verify")}
        </p>
        <div className="searchbar">
          <button className="btn" disabled={busy} onClick={() => void backupOnce()}>
            {busy ? t("savebak.saving") : info?.latest ? t("savebak.again") : t("savebak.save")}
          </button>
          <button className="btn ghost" disabled={busy} onClick={() => void refresh()}>
            {t("action.refresh")}
          </button>
        </div>

        {info !== null &&
          (info.latest ? (
            <p className="dim small">
              {t("savebak.latest", {
                when: whenText(info.latest.at_ms),
                size: sizeText(info.latest.bytes),
              })}{" "}
              {t("savebak.count", { count: info.count })}
            </p>
          ) : (
            <p className="dim small">{t("savebak.none")}</p>
          ))}
        {info !== null && <p className="dim small">{t("savebak.where", { dir: info.dir })}</p>}

        {info !== null && info.items.length > 0 && (
          <div className="savebak-list" data-savebak-list={info.items.length}>
            <p className="dim small">{t("savebak.listLead")}</p>
            <ul>
              {info.items.map((one) => (
                <li key={one.name} data-backup={one.name}>
                  <span className="savebak-when">{whenText(one.at_ms)}</span>
                  <span className="savebak-size">{sizeText(one.bytes)}</span>
                  <span className="savebak-name mono">{one.name}</span>
                  {confirm === one.name ? (
                    <>
                      <span className="dim small">{t("savebak.confirmAsk")}</span>
                      <button
                        className="btn"
                        disabled={busy}
                        onClick={() => void restore(one.name)}
                      >
                        {busy ? t("savebak.saving") : t("savebak.confirmYes")}
                      </button>
                      <button
                        className="btn ghost"
                        disabled={busy}
                        onClick={() => setConfirm(null)}
                      >
                        {t("savebak.confirmNo")}
                      </button>
                    </>
                  ) : (
                    <button
                      className="btn ghost"
                      disabled={busy}
                      onClick={() => setConfirm(one.name)}
                    >
                      {t("savebak.restore")}
                    </button>
                  )}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} />}
    </>
  );
}
