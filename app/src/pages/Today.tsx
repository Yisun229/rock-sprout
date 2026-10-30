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

  const failed = (block: string) => (digest?.degraded ?? []).includes(block);

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

      {digest?.from_cache && (
        <p className="note">{t("today.cached", { when: whenText(digest.fetched_at) })}</p>
      )}

      {!digest && <SkeletonRows rows={5} />}

      {digest && (
        <>
          <section className="card today-block" data-block="updates">
            <h2 className="card-title">{t("today.updates")}</h2>
            {digest.token_missing ? (
              <p className="dim small">{t("today.updatesNoToken")}</p>
            ) : failed("updates") ? (
              <FailedLine name={t("today.updates")} onRetry={() => void load(true)} />
            ) : digest.updates.length > 0 ? (
              <ul className="change-list">
                {digest.updates.map((m) => (
                  <li className="today-row" key={m.id}>
                    <span className="row-title">{m.name}</span>
                    <span className="badge warn">{t("today.hasUpdate")}</span>
                  </li>
                ))}
              </ul>
            ) : digest.installed_count === 0 ? (
              <p className="dim small">{t("today.updatesEmpty")}</p>
            ) : digest.checked_count === 0 ? (
              <p className="dim small">{t("today.updatesLocal")}</p>
            ) : (
              <p className="dim small">{t("today.updatesFresh")}</p>
            )}
          </section>

          <section className="card today-block" data-block="news">
            <h2 className="card-title">{t("today.news")}</h2>
            {failed("news") ? (
              <FailedLine name={t("today.news")} onRetry={() => void load(true)} />
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
              <FailedLine name={t("today.changes")} onRetry={() => void load(true)} />
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


        </>
      )}
    </section>
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

function FailedLine({ name, onRetry }: { name: string; onRetry: () => void }) {
  return (
    <p className="dim small">
      {t("today.blockFailed", { name })}
      <button className="btn ghost small today-inline" onClick={onRetry}>
        {t("today.retry")}
      </button>
    </p>
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
