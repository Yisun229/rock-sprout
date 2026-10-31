// 「今天」：一屏看完官方公告、mod.io 上刚冒出来的、你自己装了什么。
//
// 三块各取各的、各自降级：某一块没取到，其余照常显示，那一块单独写一行说明。
// 数据回来之前是骨架块——不等网络、不转圈，也不弹错误框。
import { useCallback, useEffect, useState } from "react";
import { call, type ModChange, type NewsItem, type TodayDigest } from "../api";
import SkeletonRows from "../components/states/SkeletonRows";
import { t, whenText } from "../i18n";
import { useAppStore } from "../store/appStore";

type Props = {
  /** 没有令牌时给个去处 */
  onGoSettings: () => void;
};

export default function Today({ onGoSettings }: Props) {
  const { hintCollapsed, setHintCollapsed } = useAppStore();
  const [digest, setDigest] = useState<TodayDigest | null>(null);
  const [busy, setBusy] = useState(false);
  const [warn, setWarn] = useState("");

  const load = useCallback(async (refresh: boolean) => {
    setBusy(true);
    try {
      const res = await call<TodayDigest>("today_digest", refresh ? { refresh: true } : {});
      setDigest(res);
    } catch {
      // 取整页数据失败基本是本地文件都读不了，此时保留上一次的内容更稳
    } finally {
      setBusy(false);
    }
  }, []);

  // 只有第一次进这一页才取；之后切回来直接用内存里那份，点刷新才重取
  useEffect(() => {
    if (!digest) void load(false);
  }, [digest, load]);

  /** 公告只用系统浏览器打开 http/https，别的协议一律不理 */
  async function openNews(url: string) {
    if (!/^https?:\/\//i.test(url)) return;
    try {
      await call("open_url", { url });
    } catch {
      // 打不开就算了，不值得打断阅读
    }
  }

  /** 重装 = 用本地钉死版本的包再铺一遍，不联网也要能用（M7-3） */
  async function reinstall() {
    setBusy(true);
    setWarn("");
    try {
      await call("ue4ss_install", { confirmReplace: false });
      await load(true);
    } catch (e) {
      const err = e as { code?: string; message?: string };
      if (err.code === "pkg_missing") {
        setWarn(t("today.ue4ssNeedPackage"));
      } else if (err.code === "game_running") {
        setWarn(t("today.ue4ssGameRunning"));
      } else {
        setWarn(err.message ?? t("today.ue4ssReinstallFailed"));
      }
    } finally {
      setBusy(false);
    }
  }

  const failed = (block: string) => (digest?.degraded ?? []).includes(block);
  /** 有新版才整卡置顶；没有就收成页尾一行，别拿一张空卡占最上面（Q4） */
  const hasUpdates = (digest?.updates.length ?? 0) > 0;
  /** 断网、且这次一点内容都没取到：页头统一说一句，别让三块各自喊「没取到」 */
  const offline =
    digest !== null &&
    !digest.from_cache &&
    failed("news") &&
    digest.news.length === 0 &&
    digest.mod_changes.length === 0 &&
    digest.updates.length === 0;

  /** UE4SS 装了但没生效：说一句，并给个「重装」按钮（M7-3） */
  const ue4ssBroken = digest?.ue4ss?.state === "broken";
  const ue4ssNote = digest?.ue4ss?.detail ?? "";

  return (
    <section>
      <div className="today-head">
        <div>
          <h1>{t("nav.today")}</h1>
          <p className="dim">{t("today.lead")}</p>
        </div>
        <button className="btn ghost small" disabled={busy} onClick={() => void load(true)}>
          {busy ? t("today.refreshing") : t("today.refresh")}
        </button>
      </div>

      {offline && <p className="note">{t("today.offline")}</p>}

      {/* UE4SS 失效（M7-3）：只在「装了但没生效」时出现，成功和没装都不占地方 */}
      {ue4ssBroken && (
        <div className="today-warn" data-block="ue4ss">
          <span className="hint-dot" aria-hidden="true" />
          <span className="today-warn-text">
            {t("today.ue4ssBroken")}
            {ue4ssNote && <span className="dim">（{ue4ssNote}）</span>}
          </span>
          <button className="btn small" disabled={busy} onClick={() => void reinstall()}>
            {busy ? t("today.ue4ssReinstalling") : t("today.ue4ssReinstall")}
          </button>
        </div>
      )}

      {digest?.from_cache && (
        <p className="note">{t("today.cached", { when: whenText(digest.fetched_at) })}</p>
      )}

      {warn && <p className="note warn">{warn}</p>}

      {!digest && <SkeletonRows rows={5} />}

      {digest && (
        <>
          {hasUpdates && (
            <section className="card today-block" data-block="updates">
              <h2 className="card-title">{t("today.updates")}</h2>
              <ul className="change-list">
                {digest.updates.map((m) => (
                  <li className="today-row" key={m.id}>
                    <span className="row-title">{m.name}</span>
                    <span className="badge warn">{t("today.hasUpdate")}</span>
                  </li>
                ))}
              </ul>
            </section>
          )}

          <section className="card today-block" data-block="news">
            <h2 className="card-title">{t("today.news")}</h2>
            {failed("news") ? (
              <FailedLine onRetry={() => void load(true)} />
            ) : digest.news.length === 0 ? (
              <p className="dim small">{t("today.newsEmpty")}</p>
            ) : (
              <ul className="news-list">
                {digest.news.map((n) => (
                  <NewsRow key={n.gid} item={n} onOpen={() => void openNews(n.url)} />
                ))}
              </ul>
            )}
          </section>

          <section className="card today-block" data-block="changes">
            <h2 className="card-title">{t("today.changes")}</h2>
            {digest.token_missing ? (
              <TokenHint
                collapsed={hintCollapsed}
                onGoSettings={onGoSettings}
                onCollapse={() => setHintCollapsed(true)}
                onExpand={() => setHintCollapsed(false)}
              />
            ) : failed("mod_changes") ? (
              <FailedLine onRetry={() => void load(true)} />
            ) : digest.mod_changes.length === 0 ? (
              <p className="dim small">{t("today.changesEmpty")}</p>
            ) : (
              <ul className="change-list">
                {digest.mod_changes.map((m) => (
                  <ChangeRow key={`${m.kind}-${m.id}`} item={m} />
                ))}
              </ul>
            )}
          </section>

          {!hasUpdates && <UpdatesFoot digest={digest} onRetry={() => void load(true)} />}
        </>
      )}
    </section>
  );
}

/** 没有新版时「更新」的落脚处：页尾一行小字，不占整卡（Q4 / Q9） */
function UpdatesFoot({ digest, onRetry }: { digest: TodayDigest; onRetry: () => void }) {
  if ((digest.degraded ?? []).includes("updates")) {
    return (
      <p className="today-foot" data-block="updates">
        <span className="dim small">{t("today.updates")}：</span>
        <FailedLine onRetry={onRetry} />
      </p>
    );
  }
  const text = digest.token_missing
    ? t("today.updatesNoToken")
    : digest.installed_count === 0
      ? t("today.updatesEmpty")
      : digest.checked_count === 0
        ? t("today.updatesLocal")
        : t("today.updatesFresh");
  return (
    <p className="today-foot dim small" data-block="updates">
      {t("today.updates")}：{text}
    </p>
  );
}

function NewsRow({ item, onOpen }: { item: NewsItem; onOpen: () => void }) {
  return (
    <li className="news-item">
      <button className="news-title" onClick={onOpen}>
        {item.title}
      </button>
      <span className="dim small">{whenText(item.date * 1000)}</span>
      {item.contents && <p className="news-body">{item.contents}</p>}
    </li>
  );
}

function ChangeRow({ item }: { item: ModChange }) {
  return (
    <li className="today-row">
      <span className="row-title">{item.name}</span>
      <span className={item.kind === "new" ? "badge" : "badge warn"}>
        {item.kind === "new" ? t("today.kindNew") : t("today.kindUpdated")}
      </span>
      <span className="dim small">{whenText(item.when * 1000)}</span>
    </li>
  );
}

/** 这一块没取到：正文只说「这块没取到。」，块名由上面的标题交代，不重复 */
function FailedLine({ onRetry }: { onRetry: () => void }) {
  return (
    <span className="dim small">
      {t("today.blockFailed")}
      <button className="btn ghost small today-inline" onClick={onRetry}>
        {t("today.retry")}
      </button>
    </span>
  );
}

/** 没令牌时那块位置：第一次是整张引导卡，收起后是一行小字（Q2） */
function TokenHint({
  collapsed,
  onGoSettings,
  onCollapse,
  onExpand,
}: {
  collapsed: boolean;
  onGoSettings: () => void;
  onCollapse: () => void;
  onExpand: () => void;
}) {
  if (collapsed) {
    return (
      <p className="dim small">
        {t("today.tokenCollapsed")}
        <button className="btn ghost small today-inline" onClick={onExpand}>
          {t("today.tokenExpand")}
        </button>
      </p>
    );
  }
  return (
    <div className="token-hint">
      <p className="token-hint-title">{t("today.tokenCardTitle")}</p>
      <p className="dim small">{t("today.tokenCardBody")}</p>
      <div className="token-hint-actions">
        <button
          className="btn small"
          onClick={() => {
            onCollapse();
            onGoSettings();
          }}
        >
          {t("today.tokenCardGo")}
        </button>
        <button className="btn ghost small" onClick={onCollapse}>
          {t("today.tokenCollapse")}
        </button>
      </div>
    </div>
  );
}
