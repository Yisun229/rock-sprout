// 「天赋」：24 条天赋、三个点数、每个角色当前带的那套。
//
// 只读——这一页永远不改存档。名字来自 `app/src/data/perk-names.json`，
// 查不到就显示英文名；连英文名都没有就退回 GUID 前 8 位，不猜。
import { useCallback, useEffect, useMemo, useState } from "react";
import { call, type PerkItem, type PerkName, type PerksView } from "../api";
import { t } from "../i18n";
import { useAppStore } from "../store/appStore";

/** 界面上的名字：中文优先，其次英文，最后才用编号 */
function nameOf(name: PerkName | undefined, guid: string): string {
  const zh = name?.zh?.trim();
  if (zh) return zh;
  const en = name?.en?.trim();
  if (en) return en;
  return guid.slice(0, 8);
}

/** 一条天赋的总花费：把每一级的花费加起来 */
function totalCost(one: PerkItem): number {
  return one.ranks.reduce((sum, r) => sum + r.cost, 0);
}

function PerkRow({ one, label }: { one: PerkItem; label: string }) {
  const owned = one.ownedRank !== null;
  const max = one.ranks.length;
  return (
    <li
      className={owned ? "perk-row" : "perk-row perk-row-off"}
      data-guid={one.guid}
      data-owned={owned ? "1" : "0"}
    >
      <span className="perk-name">{label}</span>
      {max === 0 ? (
        <span className="perk-meta">{t("perks.noRanks")}</span>
      ) : (
        <>
          <span className="perk-meta">
            {t("perks.rank", { rank: one.ownedRank ?? 0, max })}
          </span>
          <span className="perk-meta perk-cost">{t("perks.cost", { n: totalCost(one) })}</span>
        </>
      )}
    </li>
  );
}

export default function Perks() {
  const { page } = useAppStore();
  const [view, setView] = useState<PerksView | null>(null);
  const [busy, setBusy] = useState(true);

  const load = useCallback(async () => {
    setBusy(true);
    try {
      setView(await call<PerksView>("get_perks", {}));
    } catch {
      setView(null);
    } finally {
      setBusy(false);
    }
  }, []);

  // 页面一直挂着（切页只是 hidden），每次进这一页都重读
  useEffect(() => {
    if (page !== "perks") return;
    void load();
  }, [page, load]);

  const names = useMemo(() => {
    const map = new Map<string, PerkName>();
    for (const one of view?.all ?? []) map.set(one.guid, one.name);
    return map;
  }, [view]);

  const points = view?.points;
  const ownedCount = view?.all.filter((one) => one.ownedRank !== null).length ?? 0;

  return (
    <>
      <div className="page-head-row">
        <h1>{t("nav.perks")}</h1>
      </div>
      <p className="dim">{t("perks.lead")}</p>

      {!view && (
        <div className="card" style={{ marginTop: 16 }}>
          <p className="dim">{busy ? t("perks.loading") : t("data.notFound")}</p>
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

      {view && view.state === "ok" && points && (
        <>
          <div className="card" style={{ marginTop: 16 }} data-points={points.verified ? "ok" : "off"}>
            {points.verified ? (
              <div className="data-grid">
                <div className="data-num">
                  <div className="data-label">{t("perks.earned")}</div>
                  <div className="data-value">{points.earned ?? 0}</div>
                </div>
                <div className="data-num">
                  <div className="data-label">{t("perks.spent")}</div>
                  <div className="data-value">{points.spent ?? 0}</div>
                </div>
                <div className="data-num">
                  <div className="data-label">{t("perks.left")}</div>
                  <div className="data-value">{points.left}</div>
                </div>
              </div>
            ) : (
              // 算不全就不给数：两个来源对不上时，宁可不显示，也不凑一个看着像对的
              <p className="dim" style={{ margin: 0 }}>
                {t("perks.unverified")}
              </p>
            )}
          </div>

          <div className="card" style={{ marginTop: 16 }}>
            <div className="card-title-row">
              <span className="card-title">{t("perks.listTitle", { n: view.all.length })}</span>
              <span className="dim small">{t("perks.ownedCount", { n: ownedCount })}</span>
            </div>
            <ul className="perk-list">
              {view.all.map((one) => (
                <PerkRow key={one.guid} one={one} label={nameOf(one.name, one.guid)} />
              ))}
            </ul>
          </div>

          <div className="card" style={{ marginTop: 16 }}>
            <div className="card-title-row">
              <span className="card-title">{t("perks.loadoutsTitle")}</span>
              <span className="dim small">{t("perks.loadoutsSub", { n: view.loadouts.length })}</span>
            </div>
            <div className="perk-loadouts">
              {view.loadouts.map((one, i) => (
                <div className="perk-loadout" key={one.characterId} data-character={one.characterId}>
                  <div className="perk-loadout-head">{t("perks.character", { n: i + 1 })}</div>
                  <div className="perk-loadout-perks">
                    {one.perkGuids.length === 0
                      ? t("perks.loadoutEmpty")
                      : one.perkGuids.map((guid) => nameOf(names.get(guid), guid)).join("、")}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </>
  );
}
