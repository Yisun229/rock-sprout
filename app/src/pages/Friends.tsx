// 好友页：谁在玩深岩银河、玩了多少小时。
//
// 数据全来自 Steam Web API，只读。四种状态各有一屏：
//   no_key  —— 引导卡（去申请 + 粘进来）
//   no_id   —— 让贴主页链接
//   bad_key —— key 本身不被接受
//   offline —— 没连上；有缓存就摆缓存
import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import ErrorHint from "../ErrorHint";
import { call, toApiError, type ApiError, type FriendRow, type FriendsView } from "../api";
import { t, whenText } from "../i18n";
import { useAppStore } from "../store/appStore";

/** 分钟 → 「2.1 小时」；0 分钟写「没玩过」。 */
export function hoursText(minutes: number): string {
  if (minutes <= 0) return t("friends.never");
  const h = minutes / 60;
  return t("friends.hours", { h: h >= 100 ? Math.round(h).toString() : h.toFixed(1) });
}

function Avatar({ row }: { row: FriendRow }) {
  // 先显示名字首字，头像真的加载成功才盖上去——头像站连不上时也不会出现破图图标
  const [loaded, setLoaded] = useState(false);
  const mark = (row.name || "?").trim().slice(0, 1) || "?";
  return (
    <span className="friend-avatar-wrap">
      <span className="friend-avatar blank" aria-hidden="true">
        {mark}
      </span>
      {row.avatar ? (
        <img
          className="friend-avatar"
          data-loaded={loaded ? "1" : "0"}
          src={row.avatar}
          alt=""
          width={32}
          height={32}
          onLoad={() => setLoaded(true)}
          onError={() => setLoaded(false)}
        />
      ) : null}
    </span>
  );
}

function FriendLine({ row, owned }: { row: FriendRow; owned?: boolean }) {
  const minutes = row.playtimeMin ?? 0;
  return (
    <li className="friend-row" data-owned={owned ? "1" : "0"}>
      <Avatar row={row} />
      <span className="friend-name">{row.name || row.steamId}</span>
      <span className="friend-time mono">{hoursText(minutes)}</span>
      {owned && <span className="friend-own">{t("friends.owns")}</span>}
    </li>
  );
}

export default function Friends() {
  const [view, setView] = useState<FriendsView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [progress, setProgress] = useState<{ checked: number; total: number } | null>(null);
  const [keyDraft, setKeyDraft] = useState("");
  const [idDraft, setIdDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const { page } = useAppStore();

  const load = useCallback(async (force: boolean) => {
    setBusy(true);
    setError(null);
    setProgress(null);
    try {
      const v = await call<FriendsView>("get_friends", { force });
      setView(v);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }, []);

  // 进页面自动刷一次；10 分钟内后端会直接给缓存
  useEffect(() => {
    if (page !== "friends") return;
    void load(false);
  }, [page, load]);

  // 查询进度：后端每查完一个人发一次事件
  useEffect(() => {
    if (page !== "friends") return;
    let unlisten: (() => void) | undefined;
    void listen<{ checked: number; total: number }>("friends-progress", (e) => {
      setProgress(e.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [page]);

  async function saveKey() {
    setSaving(true);
    setError(null);
    try {
      await call("set_steam", { key: keyDraft.trim(), id: null });
      setKeyDraft("");
      await load(false);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setSaving(false);
    }
  }

  async function saveId() {
    setSaving(true);
    setError(null);
    try {
      await call("set_steam", { key: null, id: idDraft.trim() });
      setIdDraft("");
      await load(false);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setSaving(false);
    }
  }

  const state = view?.state ?? "loading";
  const visible = view?.visible ?? [];
  const hidden = view?.hidden ?? [];

  return (
    <section data-page-friends={state}>
      <div className="friends-head">
        <h1>{t("friends.title")}</h1>
        {state === "ok" || state === "offline" ? (
          <button className="btn" disabled={busy} onClick={() => void load(true)}>
            {busy ? t("friends.refreshing") : t("friends.refresh")}
          </button>
        ) : null}
      </div>
      <p className="dim">{t("friends.lead")}</p>

      {state === "loading" && <p className="dim">{t("friends.loading")}</p>}

      {state === "no_key" && (
        <div className="card" data-state="no_key">
          <div className="card-title">{t("friends.noKeyTitle")}</div>
          <p className="dim small">{t("friends.noKeyHint")}</p>
          <div className="searchbar">
            <input
              className="input mono"
              value={keyDraft}
              placeholder={t("friends.keyPlaceholder")}
              spellCheck={false}
              aria-label={t("friends.keyLabel")}
              onChange={(e) => setKeyDraft(e.target.value)}
            />
            <button className="btn" disabled={saving || keyDraft.trim() === ""} onClick={() => void saveKey()}>
              {t("friends.saveKey")}
            </button>
            <button
              className="btn ghost"
              onClick={() => void call("open_url", { url: "https://steamcommunity.com/dev/apikey" })}
            >
              {t("friends.getKey")}
            </button>
          </div>
        </div>
      )}

      {state === "no_id" && (
        <div className="card" data-state="no_id">
          <div className="card-title">{t("friends.noIdTitle")}</div>
          <p className="dim small">{t("friends.noIdHint")}</p>
          <div className="searchbar">
            <input
              className="input"
              value={idDraft}
              placeholder="https://steamcommunity.com/profiles/7656119…"
              spellCheck={false}
              aria-label={t("friends.idLabel")}
              onChange={(e) => setIdDraft(e.target.value)}
            />
            <button className="btn" disabled={saving || idDraft.trim() === ""} onClick={() => void saveId()}>
              {t("friends.saveId")}
            </button>
          </div>
        </div>
      )}

      {state === "bad_key" && (
        <div className="card" data-state="bad_key">
          <div className="card-title">{t("friends.badKeyTitle")}</div>
          <p className="dim small">{view?.note}</p>
        </div>
      )}

      {state === "offline" && (
        <div className="card" data-state="offline">
          <div className="card-title">{t("friends.offlineTitle")}</div>
          <p className="dim small">{view?.note || t("friends.offlineHint")}</p>
        </div>
      )}

      {(state === "ok" || state === "offline") && view && (
        <>
          <div className="friends-top small">
            <span>
              {t("friends.count", { n: visible.length + hidden.length, m: visible.length })}
            </span>
            {view.fetchedAt != null && (
              <span className="dim">
                {state === "offline"
                  ? t("friends.cachedAt")
                  : view.fromCache
                    ? t("friends.fromCache")
                    : t("friends.lastAt")}{" "}
                {whenText(view.fetchedAt * 1000)}
              </span>
            )}
            {progress && progress.total > 0 && (
              <span className="dim mono">
                {t("friends.progress", { checked: progress.checked, total: progress.total })}
              </span>
            )}
          </div>

          {visible.length > 0 ? (
            <ul className="friend-list" data-count={visible.length}>
              {visible.map((row) => (
                <FriendLine key={row.steamId} row={row} owned={row.ownsGame === true} />
              ))}
            </ul>
          ) : (
            <p className="dim">{t("friends.empty")}</p>
          )}

          {hidden.length > 0 && (
            <details className="friend-hidden" data-count={hidden.length}>
              <summary>
                {t("friends.hiddenTitle", { n: hidden.length })}
                <span className="dim small"> · {t("friends.hiddenHint")}</span>
              </summary>
              <ul className="friend-list">
                {hidden.map((row) => (
                  <FriendLine key={row.steamId} row={row} />
                ))}
              </ul>
            </details>
          )}
        </>
      )}

      {error && <ErrorHint error={error} />}
    </section>
  );
}
