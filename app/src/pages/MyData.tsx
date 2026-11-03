// 「我的数据」：从本地存档里读几个数，一眼看完。
//
// 只读——这一页永远不会改存档。三块之外的细节留给后面的版本，
// 底部那行 M/N 是如实交代「读到的比看到的多」。
import { useCallback, useEffect, useState } from "react";
import { call, type SaveList, type SaveOverview } from "../api";
import { t, thousands, whenText } from "../i18n";

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
  const [data, setData] = useState<SaveOverview | null>(null);
  const [list, setList] = useState<SaveList | null>(null);
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
        <p className="dim small" style={{ margin: 0 }}>
          {t("data.resources", { count: data.resources_total })}
        </p>
      </div>

      <p className="dim small" style={{ marginTop: 12 }}>
        {t("data.blocks", { read: data.blocks_read, pending: data.blocks_pending })}
        {data.from_cache ? ` · ${t("data.fromCache")}` : ""}
      </p>
    </div>
  );
}
