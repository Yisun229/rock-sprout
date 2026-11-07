// 设置页的存档备份卡：点一下把当前存档复制一份，放进数据目录的 `backups\`。
//
// 只复制、不还原——还原要处理版本冲突（游戏更新后旧档读不读得动、两份谁更新），
// 那是单独一件事。这里要做的是「动手改之前手上先有一份一模一样的存档」。
import { useCallback, useEffect, useState } from "react";
import ErrorHint from "../../ErrorHint";
import { call, toApiError, type ApiError, type SaveBackup, type SaveBackupList } from "../../api";
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
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} />}
    </>
  );
}
