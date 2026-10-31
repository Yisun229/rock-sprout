// UE4SS 卡：脚本类 mod 得先有这层运行时，不然装进去游戏也不认。
//
// 这层东西不能随软件一起发（别人写的二进制，我们只负责下载和校验），所以界面要做三件事：
// 说清楚它在干什么、下载时有进度能取消、下载不了时给一条手动下载的路。
import { useCallback, useEffect, useRef, useState } from "react";
import { toApiError, ue4ssCancel, ue4ssPrepare, ue4ssStatus } from "../../api";
import type { Ue4ssStatus } from "../../api";
import { t } from "../../i18n";

const POLL_MS = 300;

export default function Ue4ssCard() {
  const [status, setStatus] = useState<Ue4ssStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<{ message: string; hint: boolean } | null>(null);
  const alive = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const next = await ue4ssStatus();
      if (alive.current) setStatus(next);
    } catch {
      // 状态读失败不值得弹错误：下次轮询还会再读
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void refresh();
    return () => {
      alive.current = false;
    };
  }, [refresh]);

  // 下载中每 300 ms 问一次进度；下完（或失败）就停
  useEffect(() => {
    if (!status?.downloading) return;
    const timer = window.setInterval(() => void refresh(), POLL_MS);
    return () => window.clearInterval(timer);
  }, [status?.downloading, refresh]);

  async function prepare() {
    setBusy(true);
    setFailure(null);
    try {
      setStatus(await ue4ssPrepare());
    } catch (err) {
      const e = toApiError(err);
      // busy 不是用户的错，不给「手动下载」那条路
      setFailure({ message: e.message, hint: e.code !== "busy" });
    } finally {
      setBusy(false);
      void refresh();
    }
  }

  async function cancel() {
    try {
      await ue4ssCancel();
    } finally {
      void refresh();
    }
  }

  const ready = status?.package_ready === true;
  const zipReady = status?.zip_ready === true;
  const downloading = status?.downloading === true;
  const percent =
    status && status.zip_size > 0
      ? Math.min(100, Math.round((status.downloaded / status.zip_size) * 100))
      : 0;

  return (
    <div className="card" data-block="ue4ss">
      <h2>{t("ue4ss.title")}</h2>
      <p className="dim">{t("ue4ss.lead")}</p>

      {failure && (
        <div className="hint-err" role="alert">
          <span className="hint-dot" aria-hidden="true" />
          <span className="hint-text">{failure.message}</span>
        </div>
      )}

      {downloading ? (
        <>
          <p className="dim small">
            {t("ue4ss.downloading", {
              done: mb(status?.downloaded ?? 0),
              total: mb(status?.zip_size ?? 0),
            })}
          </p>
          <div className="progress" role="progressbar" aria-valuenow={percent}>
            <div className="progress-fill" style={{ width: `${percent}%` }} />
          </div>
          <button type="button" className="btn ghost" onClick={() => void cancel()}>
            {t("ue4ss.cancel")}
          </button>
        </>
      ) : ready ? (
        <p className="ue4ss-ready">{t("ue4ss.ready", { version: status?.version ?? "" })}</p>
      ) : (
        <>
          <div className="ue4ss-actions">
            <button type="button" className="btn" disabled={busy} onClick={() => void prepare()}>
              {busy ? t("ue4ss.preparing") : zipReady ? t("ue4ss.extract") : t("ue4ss.download")}
            </button>
            <span className="dim small">
              {zipReady ? t("ue4ss.zipReady") : t("ue4ss.zipMissing")}
            </span>
          </div>
          <details className="ue4ss-manual">
            <summary>{t("ue4ss.manualSummary")}</summary>
            <p className="dim small">{t("ue4ss.manualSteps")}</p>
            <p className="small">
              <a href={status?.url ?? ""} target="_blank" rel="noreferrer">
                {status?.url ?? ""}
              </a>
            </p>
            <p className="dim small">
              {t("ue4ss.manualDir")} <code>{status?.manual_dir ?? ""}</code>
            </p>
            <p className="dim small">{t("ue4ss.manualSize", { size: mb(status?.zip_size ?? 0) })}</p>
          </details>
        </>
      )}
    </div>
  );
}

/** 字节换成「12.3 MB」这种给人看的写法。 */
function mb(bytes: number): string {
  if (!bytes) return "0 MB";
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
