// 「我的数据」：从本地存档里读几个数，一眼看完。
//
// 只读——这一页永远不会改存档。三块之外的细节留给后面的版本，
// 底部那行 M/N 是如实交代「读到的比看到的多」。
import { useCallback, useEffect, useState } from "react";
import { call, type SaveDebug, type SaveList, type SaveOverview } from "../api";
import { t, thousands, whenText } from "../i18n";
import { useAppStore } from "../store/appStore";
import names from "../data/names.json";

/** 资源 id → 名字；对照表里没有的返回 null（界面显示 #id，不猜） */
function resourceName(id: string): string | null {
  const hit = (names.resources as Record<string, { en: string }>)[id];
  return hit ? hit.en : null;
}

/** 矿物块显示几条，剩下的收成「另有 N 项」 */
const RESOURCE_SHOWN = 4;

/** 秒 → 「96 小时 17 分」 */
function hoursText(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds / 60));
  return t("data.hours", { h: Math.floor(total / 60), m: total % 60 });
}

/** 选择器里那一行字：「…191195 · 141 局 · 改于 11 月 3 日 21:05」 */
function candidateText(item: SaveList["items"][number], when: string): string {
  return item.games < 0
    ? t("data.candidateBroken", { account: item.account, when })
    : t("data.candidate", { account: item.account, games: item.games, when });
}

export default function MyData() {
  const { devView } = useAppStore();
  const [data, setData] = useState<SaveOverview | null>(null);
  const [list, setList] = useState<SaveList | null>(null);
  const [debug, setDebug] = useState<SaveDebug | null>(null);
  const [busy, setBusy] = useState(true);
  const [switching, setSwitching] = useState(false);

  const load = useCallback(async () => {
    setBusy(true);
    try {
      const [overview, saves] = await Promise.all([
        call<SaveOverview>("save_overview", {}),
        call<SaveList>("save_list"),
      ]);
      setData(overview);
      setList(saves);
    } catch {
      // 真读不了（比如数据目录整个没了）就当成「没找到」，下面那三句里最贴切的一句
      setData(null);
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    if (!data) void load();
  }, [data, load]);

  // 开发者视图开着才去要那张表（关着的时候一次请求都不发）
  useEffect(() => {
    if (!devView) {
      setDebug(null);
      return;
    }
    let alive = true;
    void call<SaveDebug>("save_debug", {})
      .then((d) => {
        if (alive) setDebug(d);
      })
      .catch(() => {
        if (alive) setDebug(null);
      });
    return () => {
      alive = false;
    };
  }, [devView, data]);

  /** 换一份存档：先记住选择，再重算这一页（没缓存的那份会当场解析）。 */
  const switchTo = useCallback(async (id: string) => {
    setSwitching(true);
    try {
      await call<boolean>("set_save_file", { file: id });
      const [overview, saves] = await Promise.all([
        call<SaveOverview>("save_overview", { selected: id }),
        call<SaveList>("save_list"),
      ]);
      setData(overview);
      setList(saves);
    } finally {
      setSwitching(false);
    }
  }, []);

  if (busy && !data) {
    return (
      <div className="page-head">
        <h1>{t("nav.data")}</h1>
        <p className="dim small">{t("data.loading")}</p>
      </div>
    );
  }

  const items = list?.items ?? [];
  const current = items.find((it) => it.active) ?? null;

  // 选择器：多份才给下拉，一份就静态一行；放在标题右边，让「这些数是哪个档的」一眼可见
  const picker =
    items.length > 1 ? (
      <label className="save-pick">
        <span className="dim small">{t("data.savePick")}</span>
        <select
          value={current?.id ?? ""}
          disabled={switching}
          onChange={(e) => void switchTo(e.target.value)}
        >
          {items.map((it) => (
            <option key={it.id} value={it.id}>
              {candidateText(it, whenText(it.modified_ms))}
            </option>
          ))}
        </select>
        {switching ? <span className="dim small">{t("data.switching")}</span> : null}
      </label>
    ) : current ? (
      <span className="dim small save-only">{candidateText(current, whenText(current.modified_ms))}</span>
    ) : null;

  // 三种失败各说各的：接下来该做什么完全不一样，不能合成一句「读取失败」
  if (!data || data.state !== "ok" || !data.numbers) {
    const state = data?.state ?? "not_found";
    const note =
      state === "unknown_version"
        ? t("data.unknownVersion")
        : state === "bad_structure"
          ? t("data.badStructure")
          : t("data.notFound");
    return (
      <div className="page-head">
        <div className="page-head-row">
          <h1>{t("nav.data")}</h1>
          {picker}
        </div>
        <p className="dim">{t("data.lead")}</p>
        <div className="card" style={{ marginTop: 16 }}>
          <p className="dim">{note}</p>
          <button className="btn" onClick={() => void load()}>
            {t("data.retry")}
          </button>
        </div>
      </div>
    );
  }

  const n = data.numbers;
  const shown = data.resources.slice(0, RESOURCE_SHOWN);
  const hasUnknown = shown.some((r) => resourceName(r.id) === null);
  return (
    <div className="page-head">
      <div className="page-head-row">
        <h1>{t("nav.data")}</h1>
        {picker}
      </div>
      <p className="dim">{t("data.lead")}</p>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="data-grid">
          <div className="data-num">
            <div className="data-label">{t("data.credits")}</div>
            <div className="data-value">{thousands(n.credits)}</div>
          </div>
          <div className="data-num">
            <div className="data-label">{t("data.playtime")}</div>
            <div className="data-value">{hoursText(n.play_seconds)}</div>
          </div>
          <div className="data-num">
            <div className="data-label">{t("data.games")}</div>
            <div className="data-value">{thousands(n.games_played)}</div>
          </div>
          <div className="data-num">
            <div className="data-label">{t("data.slots")}</div>
            <div className="data-value">
              {t("data.slotsValue", { total: n.slots_total, active: n.slots_active })}
            </div>
            {n.slots_idle > 0 ? (
              <div className="dim small">{t("data.slotsIdle", { idle: n.slots_idle })}</div>
            ) : null}
          </div>
        </div>
      </div>

      <div className="card">
        <div className="card-title">{t("data.resourcesTitle")}</div>
        <ul className="res-list">
          {shown.map((r) => {
            const name = resourceName(r.id);
            return (
              <li key={r.id}>
                <span className={name ? "" : "res-unknown"}>
                  {name ?? t("data.resourceUnknown", { id: r.id })}
                </span>
                <span className="dim small"> {thousands(r.count)}</span>
              </li>
            );
          })}
        </ul>
        {hasUnknown ? (
          <p className="dim small" style={{ margin: "8px 0 0" }}>
            {t("data.resourceHint")}
          </p>
        ) : null}
        {data.resources.length > RESOURCE_SHOWN ? (
          <p className="dim small" style={{ margin: "8px 0 0" }}>
            {t("data.resourcesMore", { count: data.resources.length - RESOURCE_SHOWN })}
          </p>
        ) : null}
      </div>

      <p className="dim small" style={{ marginTop: 12 }}>
        {t("data.blocks", { read: data.blocks_read, pending: data.blocks_pending })}
        {data.from_cache ? ` · ${t("data.fromCache")}` : ""}
      </p>
      {devView ? <DebugTable debug={debug} /> : null}
    </div>
  );
}

/** 开发者视图：字段探查表 + 哈希与解析统计。只在设置页那个开关打开时出现。 */
function DebugTable({ debug }: { debug: SaveDebug | null }) {
  if (!debug) {
    return (
      <div className="card">
        <div className="card-title">{t("dev.table")}</div>
        <p className="dim small" style={{ margin: 0 }}>
          {t("dev.loading")}
        </p>
      </div>
    );
  }
  if (debug.state !== "ok") {
    return (
      <div className="card">
        <div className="card-title">{t("dev.table")}</div>
        <p className="dim small" style={{ margin: 0 }}>
          {debug.note}
        </p>
      </div>
    );
  }
  return (
    <div className="card">
      <div className="card-title">{t("dev.table")}</div>
      <p className="dim small" style={{ marginTop: 0 }}>
        {t("dev.meta", {
          bytes: thousands(debug.file_bytes),
          nodes: thousands(debug.nodes),
          consumed: thousands(debug.consumed),
          skipped: debug.skipped,
        })}
      </p>
      <p className="mono small" style={{ marginTop: 0, wordBreak: "break-all" }}>
        sha256 {debug.sha256}
      </p>
      <div className="dev-table-wrap">
        <table className="dev-table">
          <thead>
            <tr>
              <th>{t("dev.colPath")}</th>
              <th>{t("dev.colType")}</th>
              <th>{t("dev.colValue")}</th>
              <th>{t("dev.colKnown")}</th>
            </tr>
          </thead>
          <tbody>
            {debug.rows.map((r) => (
              <tr key={r.path} className={r.known ? "" : "dev-unknown"}>
                <td className="mono">{r.path}</td>
                <td className="mono dim">{r.ty}</td>
                <td className="mono">{r.value}</td>
                <td>{r.known ? t("dev.knownYes") : t("dev.knownNo")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {debug.rows_hidden > 0 ? (
        <p className="dim small" style={{ marginBottom: 0 }}>
          {t("dev.hidden", { count: debug.rows_hidden })}
        </p>
      ) : null}
      {debug.notes.length > 0 ? (
        <details style={{ marginTop: 8 }}>
          <summary className="dim small">{t("dev.notes", { count: debug.notes.length })}</summary>
          <ul className="dim small">
            {debug.notes.map((n) => (
              <li key={n} className="mono">
                {n}
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </div>
  );
}
