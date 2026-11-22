// 「天赋」：24 条天赋、三个点数、每个角色当前带的那套。
//
// 只读——这一页永远不改存档。名字来自 `app/src/data/perk-names.json`，
// 查不到就显示英文名；连英文名都没有就退回 GUID 前 8 位，不猜。
import { useCallback, useEffect, useMemo, useState } from "react";
import { call, type PerkItem, type PerkName, type PerksView } from "../api";
import perkText from "../data/perk-descriptions.json";
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

/** 游戏里的说明带 `<hi>`、`<active>` 这类富文本标签，界面上只留字 */
function plainText(text: string | undefined): string {
  if (!text) return "";
  return text
    .replace(/<\/?[^>]+>/g, "")
    .replace(/\r\n?/g, "\n")
    .trim();
}

type PerkDescEffect = {
  key?: string;
  text?: { en?: string; zh?: string };
  values?: number[];
  cleaned?: { value?: number; en?: string; zh?: string }[] | null;
  unclean?: string | null;
};

type PerkDesc = {
  asset?: string;
  title?: { en?: string; zh?: string };
  effects?: PerkDescEffect[];
};

/** 这一条天赋的说明（本地表里按 GUID 查） */
function descOf(guid: string): PerkDesc | undefined {
  const table = (perkText as { perks?: Record<string, PerkDesc> }).perks;
  return table?.[guid];
}

/** 按「已买到的等级」挑该级文案；没有分级说明就用原文 */
function textAt(one: PerkDescEffect, rank: number | null): { text: string; byRank: boolean } {
  const cleaned = one.cleaned ?? [];
  if (rank && cleaned.length > 0) {
    const pick = cleaned[Math.min(rank, cleaned.length) - 1];
    if (pick) {
      const text = plainText(pick.zh) || plainText(pick.en);
      if (text) return { text, byRank: true };
    }
  }
  return { text: plainText(one.text?.zh) || plainText(one.text?.en), byRank: false };
}

/** 悬停用的短句：第一条效果的当前等级说明 */
function hoverOf(guid: string, rank: number | null): string {
  const effects = descOf(guid)?.effects ?? [];
  const first = effects.map((one) => textAt(one, rank).text).find((text) => text.length > 0);
  return first ?? "";
}

/** 一条天赋的总花费：把每一级的花费加起来 */
function totalCost(one: PerkItem): number {
  return one.ranks.reduce((sum, r) => sum + r.cost, 0);
}

function PerkRow({
  one,
  label,
  selected,
  onPick,
}: {
  one: PerkItem;
  label: string;
  selected: boolean;
  onPick: (guid: string) => void;
}) {
  const owned = one.ownedRank !== null;
  const max = one.ranks.length;
  const hover = hoverOf(one.guid, one.ownedRank);
  return (
    <li
      className={
        owned
          ? selected
            ? "perk-row perk-row-on"
            : "perk-row"
          : selected
            ? "perk-row perk-row-off perk-row-on"
            : "perk-row perk-row-off"
      }
      data-guid={one.guid}
      data-owned={owned ? "1" : "0"}
      title={hover || undefined}
      onClick={() => onPick(one.guid)}
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
  // 历史预设：默认落在第一栏「有内容」的，全空就停在第一栏
  const [slot, setSlot] = useState<number | null>(null);
  // 点中的天赋：在点数区下面展开说明（悬停也能看到一句）
  const [picked, setPicked] = useState<string | null>(null);

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
  const slots = view?.slots ?? [];
  const activeSlot =
    slot ?? slots.find((one) => one.entries.length > 0)?.slot ?? slots[0]?.slot ?? null;
  const shownSlot = slots.find((one) => one.slot === activeSlot) ?? null;

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

          <div className="card" style={{ marginTop: 16 }} data-perk-desc={picked ?? ""}>
            <div className="card-title-row">
              <span className="card-title">{t("perks.descTitle")}</span>
              <span className="dim small">{t("perks.descHint")}</span>
            </div>
            {!picked && <p className="dim small" style={{ margin: 0 }}>{t("perks.descPick")}</p>}
            {picked && (
              <div className="perk-desc" data-desc-guid={picked}>
                <div className="perk-desc-head">
                  {nameOf(names.get(picked), picked)}
                  {(() => {
                    const one = view.all.find((x) => x.guid === picked);
                    const rank = one?.ownedRank ?? null;
                    return rank ? (
                      <span className="dim small"> · {t("perks.descRank", { n: rank })}</span>
                    ) : (
                      <span className="dim small"> · {t("perks.descNoRank")}</span>
                    );
                  })()}
                </div>
                {(descOf(picked)?.effects ?? []).length === 0 && (
                  <p className="dim small" style={{ margin: 0 }}>
                    {t("perks.descMissing")}
                  </p>
                )}
                {(descOf(picked)?.effects ?? []).map((one, i) => {
                  const rank = view.all.find((x) => x.guid === picked)?.ownedRank ?? null;
                  const shown = textAt(one, rank);
                  return (
                    <p className="perk-desc-line" key={one.key ?? i} data-by-rank={shown.byRank ? "1" : "0"}>
                      {shown.text || t("perks.descMissing")}
                    </p>
                  );
                })}
              </div>
            )}
          </div>

          <div className="card" style={{ marginTop: 16 }}>
            <div className="card-title-row">
              <span className="card-title">{t("perks.listTitle", { n: view.all.length })}</span>
              <span className="dim small">{t("perks.ownedCount", { n: ownedCount })}</span>
            </div>
            <ul className="perk-list">
              {view.all.map((one) => (
                <PerkRow
                  key={one.guid}
                  one={one}
                  label={nameOf(one.name, one.guid)}
                  selected={picked === one.guid}
                  onPick={(guid) => setPicked((old) => (old === guid ? null : guid))}
                />
              ))}
            </ul>
          </div>

          <div className="card" style={{ marginTop: 16 }} data-loadouts={view.loadouts.length}>
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

          {slots.length > 0 && (
            <div className="card" style={{ marginTop: 16 }} data-perk-slots={slots.length}>
              <div className="card-title-row">
                <span className="card-title">{t("perks.slotsTitle")}</span>
                <span className="dim small">{t("perks.slotsSub", { n: slots.length })}</span>
              </div>
              <p className="dim small">{t("perks.slotsLead")}</p>
              <div className="perk-slots">
                {slots.map((one) => (
                  <button
                    key={one.slot}
                    type="button"
                    className={one.slot === activeSlot ? "btn perk-slot-on" : "btn"}
                    data-slot={one.slot}
                    onClick={() => setSlot(one.slot)}
                  >
                    {t("perks.slotName", { n: one.slot })}
                    <span className="dim small">
                      {one.entries.length > 0 ? ` · ${one.entries.length}` : ""}
                    </span>
                  </button>
                ))}
              </div>
              <div className="perk-loadouts" data-slot-shown={activeSlot ?? 0}>
                {shownSlot && shownSlot.entries.length > 0 ? (
                  shownSlot.entries.map((one, i) => (
                    <div className="perk-loadout" key={one.characterId}>
                      <div className="perk-loadout-head">{t("perks.character", { n: i + 1 })}</div>
                      <div className="perk-loadout-perks">
                        {one.perkGuids.length === 0
                          ? t("perks.loadoutEmpty")
                          : one.perkGuids.map((guid) => nameOf(names.get(guid), guid)).join("、")}
                      </div>
                    </div>
                  ))
                ) : (
                  <p className="dim small">{t("perks.slotEmpty")}</p>
                )}
              </div>
            </div>
          )}
        </>
      )}
    </>
  );
}
