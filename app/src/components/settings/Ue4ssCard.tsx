// UE4SS 卡：脚本类 mod 得先有这层运行时，不然装进去游戏也不认。
//
// 三件事分三段：把包弄到本地（下载/校验）、铺进游戏目录（撞名要问一次）、撤回来（原样放回去）。
// 这层东西不能随软件一起发——别人写的二进制，我们只负责下载、校验、摆到该在的位置。
import { useCallback, useEffect, useRef, useState } from "react";
import {
  toApiError,
  ue4ssCancel,
  ue4ssGameStatus,
  ue4ssInstall,
  ue4ssPrepare,
  ue4ssStatus,
  ue4ssUninstall,
} from "../../api";
import type { Ue4ssExisting, Ue4ssStatus } from "../../api";
import { t } from "../../i18n";

const POLL_MS = 300;

export default function Ue4ssCard() {
  const [status, setStatus] = useState<Ue4ssStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<{ message: string; hint: boolean } | null>(null);
  const [game, setGame] = useState<Ue4ssExisting | null>(null);
  const [conflict, setConflict] = useState<Ue4ssExisting | null>(null);
  const [gameBusy, setGameBusy] = useState(false);
  const [gameError, setGameError] = useState<string | null>(null);
  const [restored, setRestored] = useState(false);
  const alive = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const next = await ue4ssStatus();
      if (alive.current) setStatus(next);
    } catch {
      // 状态读失败不值得弹错误：下次轮询还会再读
    }
  }, []);

  const refreshGame = useCallback(async () => {
    try {
      const next = await ue4ssGameStatus();
      if (alive.current) setGame(next);
    } catch {
      if (alive.current) setGame(null);
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void refresh();
    void refreshGame();
    return () => {
      alive.current = false;
    };
  }, [refresh, refreshGame]);

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

  async function install(confirmReplace: boolean) {
    setGameBusy(true);
    setGameError(null);
    setRestored(false);
    try {
      const outcome = await ue4ssInstall(confirmReplace);
      setGame(outcome.existing);
      setConflict(null);
    } catch (err) {
      const e = toApiError(err);
      if (e.code === "ue4ss_conflict") {
        setConflict(game ?? { kind: "other", detail: e.message });
      } else {
        setGameError(e.message);
      }
    } finally {
      setGameBusy(false);
    }
  }

  async function remove() {
    setGameBusy(true);
    setGameError(null);
    try {
      const outcome = await ue4ssUninstall();
      setGame(outcome.existing);
      setRestored(true);
    } catch (err) {
      setGameError(toApiError(err).message);
    } finally {
      setGameBusy(false);
    }
  }

  const ready = status?.package_ready === true;
  const zipReady = status?.zip_ready === true;
  const downloading = status?.downloading === true;
  const percent =
    status && status.zip_size > 0
      ? Math.min(100, Math.round((status.downloaded / status.zip_size) * 100))
      : 0;
  const kind = game?.kind ?? null;

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

      {/* 铺进游戏目录这一段 */}
      <div className="ue4ss-game" data-block="ue4ss-game">
        <h3>{t("ue4ss.gameTitle")}</h3>
        {kind === null ? (
          <p className="dim small">{t("ue4ss.gameNoGame")}</p>
        ) : (
          <>
            <p className={kind === "other" ? "small" : "dim small"}>
              {kind === "none" ? t("ue4ss.gameNone") : kind === "ours" ? t("ue4ss.gameOurs") : t("ue4ss.gameOther")}
            </p>
            {kind === "other" && <p className="dim small">{game?.detail}</p>}
            {restored && <p className="ue4ss-ready">{t("ue4ss.restored")}</p>}
            {gameError && (
              <div className="hint-err" role="alert">
                <span className="hint-dot" aria-hidden="true" />
                <span className="hint-text">{gameError}</span>
              </div>
            )}
            <div className="ue4ss-actions">
              {kind === "ours" ? (
                <button type="button" className="btn ghost" disabled={gameBusy} onClick={() => void remove()}>
                  {gameBusy ? t("ue4ss.removing") : t("ue4ss.remove")}
                </button>
              ) : (
                <button
                  type="button"
                  className="btn"
                  disabled={gameBusy || !ready}
                  title={ready ? undefined : t("ue4ss.needPackage")}
                  onClick={() => void install(false)}
                >
                  {gameBusy ? t("ue4ss.installing") : t("ue4ss.install")}
                </button>
              )}
            </div>
          </>
        )}
      </div>

      {/* 撞名弹窗：先问一次，同意才动别人的文件 */}
      {conflict && (
        <div className="modal" role="dialog" aria-modal="true" aria-label={t("ue4ss.conflictTitle")}>
          <div className="modal-card">
            <h3 className="modal-title">{t("ue4ss.conflictTitle")}</h3>
            <p>{t("ue4ss.conflictBody")}</p>
            <p className="dim small">{conflict.detail}</p>
            <div className="modal-actions">
              <button type="button" className="btn ghost" onClick={() => setConflict(null)}>
                {t("ue4ss.conflictCancel")}
              </button>
              <button type="button" className="btn" onClick={() => void install(true)}>
                {t("ue4ss.conflictOk")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/** 字节换成「12.3 MB」这种给人看的写法。 */
function mb(bytes: number): string {
  if (!bytes) return "0 MB";
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
