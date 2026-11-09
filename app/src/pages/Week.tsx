// 「本周」：赛季进度、深潜、战役。
//
// 只读——这一页永远不改存档。
//
// 三条显示规矩：
// 1. 赛季**不显示等级**：等级由游戏运行时累计的 PP 决定，存档里没有（见交接 §5.3）。
//    只显示 `XP` 与「已领 N / 100 条奖励」。
// 2. 挑战文案由后端填好；填不上时后端给 `null`，这里只显示进度数字，绝不拼半句。
// 3. 表外 GUID 不报错、不隐藏：显示前 8 位，悬停看全串，点一下复制。
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { call, type CampaignProgress, type ChallengeProgress, type ProgressView, type SeasonProgress } from "../api";
import { t } from "../i18n";
import { useAppStore } from "../store/appStore";

/** 大数字折成「24.7 万」这种，读起来快，也不占地方 */
function wan(n: number): string {
  if (n >= 10000) return `${(n / 10000).toFixed(1).replace(/\.0$/, "")} 万`;
  return String(n);
}

function Section({
  title,
  sub,
  open,
  onToggle,
  children,
}: {
  title: string;
  sub: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <section className="week-section" data-open={open ? "1" : "0"}>
      <button className="week-head" onClick={onToggle}>
        <span className="card-title">{title}</span>
        <span className="dim small">{sub}</span>
        <span className="week-toggle">{open ? t("week.collapse") : t("week.expand")}</span>
      </button>
      {open && <div className="card week-body">{children}</div>}
    </section>
  );
}

function ChallengeRow({ one }: { one: ChallengeProgress }) {
  // 后端填不上短名时 text 为 null：这里只给能确定的进度，不给半句
  const pct = one.total > 0 ? Math.min(100, Math.round((one.wins / one.total) * 100)) : 0;
  return (
    <li className="week-challenge" data-done={one.done ? "1" : "0"}>
      <div className="week-challenge-line">
        <span className={one.text ? "week-challenge-text" : "week-challenge-text dim"}>
          {one.text ?? t("week.challengeNoText")}
        </span>
        <span className="week-challenge-num mono">
          {one.wins} / {one.total}
        </span>
        {one.done && <span className="badge">{t("week.done")}</span>}
      </div>
      {one.total > 0 && (
        <div className="week-bar">
          <i style={{ width: `${pct}%`, animation: "none" }} />
        </div>
      )}
    </li>
  );
}

function SeasonBlock({ one }: { one: SeasonProgress }) {
  return (
    <div className="week-season" data-season={one.id}>
      <div className="week-season-head">
        <h3 className="week-season-name">{one.name.zh}</h3>
        <span className="dim small">{one.name.en}</span>
      </div>
      <p className="week-season-stat">
        {t("week.seasonStats", { xp: wan(one.xp), claimed: one.claimed, total: one.total })}
      </p>
      {one.challenges.length === 0 ? (
        <p className="dim small">{t("week.noChallenges")}</p>
      ) : (
        <ul className="week-challenges">
          {one.challenges.map((c) => (
            <ChallengeRow key={c.id} one={c} />
          ))}
        </ul>
      )}
    </div>
  );
}

/** 表外战役：只剩编号，显示前 8 位，点一下复制全串 */
function UnknownCampaign({ one }: { one: CampaignProgress }) {
  return (
    <li className="week-campaign" data-unknown="1">
      <button
        className="mono week-unknown-id"
        title={t("week.copyId")}
        onClick={() => {
          void navigator.clipboard?.writeText(one.id);
        }}
      >
        {one.id.slice(0, 8)}
      </button>
    </li>
  );
}

function CampaignRow({ one }: { one: CampaignProgress }) {
  if (one.unknown) return <UnknownCampaign one={one} />;
  return (
    <li className="week-campaign" data-retired={one.retired ? "1" : "0"}>
      <span>{one.name?.zh || one.name?.en || ""}</span>
      {one.retired && <span className="badge">{t("week.retired")}</span>}
    </li>
  );
}

export default function Week() {
  const { page } = useAppStore();
  const [view, setView] = useState<ProgressView | null>(null);
  const [busy, setBusy] = useState(true);
  const [seasonOpen, setSeasonOpen] = useState(true);
  const [diveOpen, setDiveOpen] = useState(false);
  const [campaignOpen, setCampaignOpen] = useState(false);

  const load = useCallback(async () => {
    setBusy(true);
    try {
      setView(await call<ProgressView>("progress_view", {}));
    } catch {
      setView(null);
    } finally {
      setBusy(false);
    }
  }, []);

  // 页面一直挂着（切页只是 hidden），每次进这一页都重读
  useEffect(() => {
    if (page !== "week") return;
    void load();
  }, [page, load]);

  const known = view?.campaigns.completed.filter((c) => !c.unknown) ?? [];
  const removed = view?.campaigns.completed.filter((c) => c.unknown) ?? [];

  return (
    <>
      <div className="page-head-row">
        <h1>{t("nav.week")}</h1>
      </div>
      <p className="dim">{t("week.lead")}</p>

      {!view && (
        <div className="card" style={{ marginTop: 16 }}>
          <p className="dim">{busy ? t("week.loading") : t("data.notFound")}</p>
          {!busy && (
            <button className="btn" onClick={() => void load()}>
              {t("data.retry")}
            </button>
          )}
        </div>
      )}

      {view && view.state !== "ok" && (
        <div className="card" style={{ marginTop: 16 }}>
          <p className="dim">{view.note || t("data.notFound")}</p>
          <button className="btn" onClick={() => void load()}>
            {t("data.retry")}
          </button>
        </div>
      )}

      {view && view.state === "ok" && (
        <div style={{ marginTop: 16 }}>
          <Section
            title={t("week.seasonTitle")}
            sub={t("week.seasonSub", { n: view.seasons.length })}
            open={seasonOpen}
            onToggle={() => setSeasonOpen((v) => !v)}
          >
            {view.seasons.length === 0 ? (
              <p className="dim small" style={{ margin: 0 }}>
                {t("week.none")}
              </p>
            ) : (
              view.seasons.map((s) => <SeasonBlock key={s.id} one={s} />)
            )}
          </Section>

          <Section
            title={t("week.diveTitle")}
            sub={t("week.diveSub")}
            open={diveOpen}
            onToggle={() => setDiveOpen((v) => !v)}
          >
            <dl className="week-dives">
              <dt>{t("week.diveNormal")}</dt>
              <dd>
                {view.deepDives.normal.completed ? t("week.diveDone") : t("week.diveNotDone")}
              </dd>
              <dt>{t("week.diveElite")}</dt>
              <dd>
                {view.deepDives.elite.progress > 0
                  ? t("week.diveStage", { n: view.deepDives.elite.progress })
                  : t("week.diveNotDone")}
              </dd>
            </dl>
            <p className="dim small" style={{ marginBottom: 0 }}>
              {t("week.diveNote")}
            </p>
          </Section>

          <Section
            title={t("week.campaignTitle")}
            sub={t("week.campaignSub", { n: known.length })}
            open={campaignOpen}
            onToggle={() => setCampaignOpen((v) => !v)}
          >
            {known.length === 0 ? (
              <p className="dim small" style={{ margin: 0 }}>
                {t("week.none")}
              </p>
            ) : (
              <ul className="week-campaigns">
                {known.map((c) => (
                  <CampaignRow key={c.id} one={c} />
                ))}
              </ul>
            )}
            {removed.length > 0 && (
              <>
                <h4 className="week-removed-title">{t("week.removedTitle", { n: removed.length })}</h4>
                <p className="dim small" style={{ marginTop: 0 }}>
                  {t("week.removedHint")}
                </p>
                <ul className="week-campaigns">
                  {removed.map((c) => (
                    <CampaignRow key={c.id} one={c} />
                  ))}
                </ul>
              </>
            )}
          </Section>
        </div>
      )}
    </>
  );
}
