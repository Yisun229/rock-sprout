// 「今天」页的酒吧块：酒单按组排开，每条写清「能不能点」卡在哪一层。
//
// 三层判定是后端算好再送上来的（执照 / 材料 / 都够），这里只负责说人话。
// 八款特调按组并列，不标「今天的就是这款」——存档里没有这个信息。
import { useCallback, useEffect, useState } from "react";
import { call, toApiError, type ApiError, type BarRow, type BarView } from "../../api";
import ErrorHint from "../../ErrorHint";
import { t } from "../../i18n";

const GROUPS: { key: string; title: string; hint?: string }[] = [
  { key: "group", title: "集团出品" },
  { key: "special", title: "今日特调", hint: "八款共用一张执照；当天轮到哪一款，存档里没有记录。" },
  { key: "crafted", title: "精酿" },
  { key: "seasonal", title: "季节活动", hint: "活动期才能买；存档里没有活动状态。" },
];

/** 「3 个麦芽星果」；信用点不带「个」。 */
function costText(one: { name: string; need: number }): string {
  return one.name === "信用点" ? `${one.need} 信用点` : `${one.need} 个${one.name}`;
}

function stateText(row: BarRow): string {
  if (row.state === "ok") return t("today.bar.ok");
  if (row.state === "license") return t("today.bar.license");
  const sh = row.short;
  if (!sh) return t("today.bar.short");
  if (sh.name === "信用点") return t("today.bar.shortCredits", { n: sh.need - sh.have });
  return t("today.bar.shortItem", { n: sh.need - sh.have, name: sh.name });
}

function Row({ row }: { row: BarRow }) {
  return (
    <li className="bar-row" data-guid={row.id} data-state={row.state}>
      <div className="bar-main">
        <span className="bar-name">{row.name}</span>
        <span className={`bar-state bar-state-${row.state}`}>{stateText(row)}</span>
      </div>
      <div className="bar-meta">
        <span className="bar-cost">{row.needs.map(costText).join(" · ")}</span>
        {row.rank !== null && row.rank > 1 && (
          <span className="bar-rank">{t("today.bar.rank", { n: row.rank })}</span>
        )}
      </div>
    </li>
  );
}

export default function BarBlock() {
  const [view, setView] = useState<BarView | null>(null);
  const [error, setError] = useState<ApiError | null>(null);

  const load = useCallback(async () => {
    try {
      setView(await call<BarView>("bar_view", {}));
      setError(null);
    } catch (e) {
      setError(toApiError(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (error) {
    return (
      <section className="card today-block" data-block="bar">
        <h2 className="card-title">{t("today.bar.title")}</h2>
        <ErrorHint error={error} />
      </section>
    );
  }

  if (!view) {
    return (
      <section className="card today-block" data-block="bar">
        <h2 className="card-title">{t("today.bar.title")}</h2>
        <p className="dim small">{t("today.bar.loading")}</p>
      </section>
    );
  }

  if (view.state !== "ok") {
    return (
      <section className="card today-block" data-block="bar">
        <h2 className="card-title">{t("today.bar.title")}</h2>
        <p className="dim small">{view.note ?? t("today.bar.none")}</p>
      </section>
    );
  }

  return (
    <section className="card today-block" data-block="bar" data-ready={view.ready} data-total={view.total}>
      <div className="today-head">
        <h2 className="card-title">
          {t("today.bar.title")}
          <span className="bar-count">{t("today.bar.count", { ready: view.ready, total: view.total })}</span>
        </h2>
        <span className="bar-credits mono" data-credits={view.credits}>
          {t("today.bar.credits", { n: view.credits.toLocaleString("en-US") })}
          <span className="dim"> · {t("today.bar.maxLevel", { max: view.maxLevel })}</span>
        </span>
      </div>

      <ul className="bar-stock">
        {view.ingredients.map((i) => (
          <li key={i.id} className="bar-stock-item" data-hold={i.have}>
            <span className="bar-stock-name">{i.name}</span>
            <span className="bar-stock-num mono">{i.have}</span>
          </li>
        ))}
      </ul>

      {GROUPS.map((g) => {
        const rows = view.rows.filter((r) => r.group === g.key);
        if (rows.length === 0) return null;
        return (
          <div className="bar-group" key={g.key} data-group={g.key}>
            <h3 className="bar-group-title">{g.title}</h3>
            {g.hint && <p className="dim small bar-group-hint">{g.hint}</p>}
            <ul className="bar-list">
              {rows.map((r) => (
                <Row key={r.id} row={r} />
              ))}
            </ul>
          </div>
        );
      })}
    </section>
  );
}
