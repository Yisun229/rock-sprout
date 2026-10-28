// 设置页的第四张卡：现场备份（快照）。
//
// 每次装/卸/导入之前后端会自动拍一份，这里顺手也能自己存一个保存点。
// 出事的时候点「退回到这里」，合并包和「装了哪些 mod」一起退回去。
import { useEffect, useRef, useState } from "react";
import ErrorHint from "../../ErrorHint";
import {
  call,
  toApiError,
  type ApiError,
  type GameCheck,
  type RollbackOutcome,
  type Snapshot,
} from "../../api";
import { t, whenText } from "../../i18n";
import ConfirmDialog from "../ConfirmDialog";

/** 把字节数说成人话（快照一般 2–5 MB）。 */
function sizeText(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

export default function SnapshotCard() {
  const [items, setItems] = useState<Snapshot[] | null>(null);
  const [busy, setBusy] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const [asking, setAsking] = useState<Snapshot | null>(null);
  const [blocked, setBlocked] = useState(false);
  const started = useRef(false);

  async function refresh() {
    try {
      const list = await call<Snapshot[]>("list_snapshots");
      setItems(list);
    } catch (e) {
      setError(toApiError(e));
    }
  }

  // 游戏本体更新过的话，旧快照里装的是上一版游戏的数据，硬退回去等于投毒
  async function checkGame() {
    try {
      const g = await call<GameCheck>("game_status");
      setBlocked(g.changed);
    } catch {
      setBlocked(false);
    }
  }

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void refresh();
    void checkGame();
  }, []);

  async function savePoint() {
    setBusy("save");
    setError(null);
    setNote("");
    try {
      const snap = await call<Snapshot>("create_snapshot", { reason: "手动保存点" });
      setNote(t("snap.saved", { reason: snap.meta.reason }));
      await refresh();
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy("");
    }
  }

  async function doRollback(snap: Snapshot) {
    setBusy(snap.id);
    setError(null);
    setNote("");
    try {
      const res = await call<RollbackOutcome>("rollback_snapshot", { id: snap.id });
      setNote(t("snap.rolledBack", { count: res.mods }));
      await refresh();
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy("");
      setAsking(null);
    }
  }

  async function remove(snap: Snapshot) {
    setBusy(snap.id);
    setError(null);
    setNote("");
    try {
      await call<string>("delete_snapshot", { id: snap.id });
      setNote(t("snap.deleted"));
      await refresh();
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy("");
    }
  }

  return (
    <>
      <div className="card">
        <div className="card-title">{t("snap.title")}</div>
        <p className="dim small">{t("snap.hint")}</p>
        <div className="searchbar">
          <button
            className="btn"
            disabled={busy !== ""}
            onClick={() => void savePoint()}
          >
            {busy === "save" ? t("snap.saving") : t("snap.save")}
          </button>
          <button
            className="btn ghost"
            disabled={busy !== ""}
            onClick={() => void refresh()}
          >
            {t("action.refresh")}
          </button>
        </div>

        {items !== null && items.length === 0 && (
          <p className="dim small">{t("snap.empty")}</p>
        )}
        {blocked && <p className="note err">{t("snap.rollbackBlocked")}</p>}
        {items !== null && items.length > 0 && (
          <div className="list">
            {items.map((snap) => (
              <div className="row" key={snap.id}>
                <div className="row-main">
                  <div className="row-title">
                    {t("snap.item", {
                      when: whenText(snap.meta.created_at),
                      reason: snap.meta.reason,
                    })}
                    {snap.meta.manual && (
                      <span className="badge">{t("snap.manual")}</span>
                    )}
                  </div>
                  <div className="row-sub">
                    {t("snap.size", {
                      count: snap.meta.mods,
                      size: sizeText(snap.meta.pak_bytes),
                    })}
                  </div>
                </div>
                <div className="row-side">
                  <button
                    className="btn small"
                    disabled={busy !== "" || blocked}
                    onClick={() => setAsking(snap)}
                  >
                    {busy === snap.id ? t("snap.rollingBack") : t("snap.rollback")}
                  </button>
                  <button
                    className="btn small ghost"
                    disabled={busy !== ""}
                    onClick={() => void remove(snap)}
                  >
                    {t("snap.delete")}
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} />}

      {asking && (
        <ConfirmDialog
          title={t("snap.rollback")}
          lines={[
            t("snap.item", {
              when: whenText(asking.meta.created_at),
              reason: asking.meta.reason,
            }),
            t("snap.size", {
              count: asking.meta.mods,
              size: sizeText(asking.meta.pak_bytes),
            }),
            t("snap.hint"),
          ]}
          confirmLabel={t("snap.rollback")}
          cancelLabel={t("snap.cancel")}
          busy={busy === asking.id}
          onCancel={() => setAsking(null)}
          onConfirm={() => void doRollback(asking)}
        />
      )}
    </>
  );
}
