// 「装备」：每个角色槽三套配装，每套八个格子。
//
// 只读——这一页永远不改存档。名字来自 `app/src/data/weapon-names.json`，
// 查不到的格子就显示 `#编号前 8 位`，不猜。
import { useCallback, useEffect, useState } from "react";
import { call, type CharacterEquip, type EquipmentItem, type EquipmentView } from "../api";
import { t, thousands } from "../i18n";
import { classLabel } from "../classNames";
import { NameSection } from "../components/NameSection";
import type { NestedVocab } from "../components/NameSection";
import BoscoBlock from "../components/equip/BoscoBlock";
import { useAppStore } from "../store/appStore";

/** i18n 的键类型（避免把随便一个字符串当键传进来） */
type MsgKey = Parameters<typeof t>[0];

/** 八格的固定标签 */
const SLOT_TEXT: Record<string, MsgKey> = {
  PrimaryWeapon: "equip.slotPrimary",
  SecondaryWeapon: "equip.slotSecondary",
  TraversalTool: "equip.slotTraversal",
  ClassTool: "equip.slotClassTool",
  Armor: "equip.slotArmor",
  Flare: "equip.slotFlare",
  MiningTool: "equip.slotMining",
  Grenade: "equip.slotGrenade",
};

function classText(one: CharacterEquip): string {
  return classLabel(one.class) ?? t("equip.unknownClass", { slot: one.slot });
}

// 十字镐的两层：大组固定这个顺序（其他放最后），组内部位也按固定顺序排。
// 词表现在从组件外传进去——抽组件时只参数化，行为不变。
const PICKAXE_VOCAB: NestedVocab = {
  groups: ["刀刃", "手柄", "其他"],
  parts: ["前刃", "后刃", "主刃", "特刃", "握柄", "柄身", "镐头", "材质", "尾椎"],
};

export default function Equipment() {
  const { page } = useAppStore();
  const [view, setView] = useState<EquipmentView | null>(null);
  const [busy, setBusy] = useState(true);
  const [open, setOpen] = useState<EquipmentItem | null>(null);

  const load = useCallback(async () => {
    setBusy(true);
    try {
      setView(await call<EquipmentView>("equipment_view", {}));
    } catch {
      setView(null);
    } finally {
      setBusy(false);
    }
  }, []);

  // 页面一直挂着（切页只是 hidden），每次进这一页都重读
  useEffect(() => {
    if (page !== "equip") return;
    void load();
  }, [page, load]);

  // Esc 关抽屉
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  const state = view?.state ?? "not_found";
  const bad = state !== "ok";

  return (
    <>
      <div className="page-head-row">
        <h1>{t("equip.title")}</h1>
        {view?.state === "ok" && (
          <span className="dim small">
            {view.from_cache ? t("equip.fromCache") : t("equip.fresh")}
          </span>
        )}
      </div>
      <p className="lead">{t("equip.lead")}</p>

      {view?.state === "ok" && (
        <>
          <p className="equip-summary" data-summary="1">
            {[
              [t("equip.sumOverclocks"), view.summary.overclocksOwned, view.summary.overclocksTotal],
              [t("equip.sumSkins"), view.summary.skinsOwned, view.summary.skinsTotal],
              // 博斯科的涂装不在这里：它进了下面的博斯科块，分母也不同（45 而不是 40）
              [t("equip.sumPickaxe"), view.summary.pickaxeOwned, view.summary.pickaxeTotal],
            ]
              .map(([label, owned, total]) =>
                t("equip.sumItem", {
                  label,
                  owned,
                  total,
                  pct: total ? Math.round((Number(owned) * 100) / Number(total)) : 0,
                }),
              )
              .join(" · ")}
          </p>
          <p className="equip-summary">
            {t("equip.sumVanity", { level: view.summary.vanityLevel, xp: thousands(view.summary.vanityXp) })}
            {" · "}
            {t("equip.sumVanityItems", { n: view.summary.vanityOwned })}
          </p>
          <p className="dim small" style={{ margin: "-6px 0 12px" }}>
            {t("equip.sumNote")}
          </p>
        </>
      )}

      {busy && !view && <p className="dim small">{t("equip.loading")}</p>}

      {bad && view && (
        <div className="card">
          <p style={{ margin: 0 }}>{view.note || t("equip.notFound")}</p>
          <button className="btn small" style={{ marginTop: 10 }} onClick={() => void load()}>
            {t("equip.retry")}
          </button>
        </div>
      )}

      {view?.state === "ok" && (
        <h2 className="page-section" data-section-head="characters">
          {t("equip.secCharacters")}
        </h2>
      )}

      {view?.state === "ok" &&
        view.characters
          .filter((one) => !one.bosco)
          .map((one) => (
          <div className="card" key={one.slot} data-char={one.slot}>
            <div className="card-title-row">
              <span className="card-title">{classText(one)}</span>
              {!one.class && <span className="dim small">{t("equip.unknownHint")}</span>}
            </div>

            {(() => {
              // 每人的配装套数不一样（这份存档 1～6 套，不是固定三套），
              // 一套都没装的就收起来，末尾交代一句，别摆一排空格子。
              const used = one.loadouts.filter((s) => s.items.some((i) => i.id !== ""));
              const blank = one.loadouts.length - used.length;
              if (used.length === 0) {
                return <p className="dim small">{t("equip.noLoadout")}</p>;
              }
              return (
                <>
                  {used.map((set) => (
                    <div className="equip-set" key={set.index}>
                      <div className="equip-set-label">{t("equip.loadout", { n: set.index })}</div>
                      <div className="equip-grid">
                        {set.items.map((item) => (
                          <button
                            key={item.key}
                            className={
                              item.id === ""
                                ? "equip-cell empty"
                                : item.known
                                  ? "equip-cell"
                                  : "equip-cell unknown"
                            }
                            disabled={item.id === ""}
                            onClick={() => setOpen(item)}
                            data-slot={item.key}
                          >
                            <span className="equip-slot">
                              {t(SLOT_TEXT[item.key] ?? "equip.slotOther")}
                            </span>
                            <span className="equip-name">
                              {item.id === ""
                                ? t("equip.emptySlot")
                                : item.name === t(SLOT_TEXT[item.key] ?? "equip.slotOther")
                                  ? ""
                                  : item.name}
                            </span>
                          </button>
                        ))}
                      </div>
                    </div>
                  ))}
                  {blank > 0 && (
                    <p className="dim small" style={{ marginTop: 10, marginBottom: 0 }}>
                      {t("equip.blankSets", { n: blank })}
                    </p>
                  )}
                </>
              );
            })()}
          </div>
        ))}

      {view?.state === "ok" && (
        <>
          <h2 className="page-section" data-section-head="bosco">
            {t("equip.boscoTitle")}
          </h2>
          <BoscoBlock />
        </>
      )}

      {view?.state === "ok" && view.pickaxe.length > 0 && (
        <h2 className="page-section" data-section-head="pickaxe">
          {t("equip.pickaxeTitle")}
        </h2>
      )}

      {view?.state === "ok" && view.pickaxe.length > 0 && (
        <NameSection
          title={t("equip.pickaxeTitle")}
          ownedLabel={t("equip.pickaxeOwned", { n: view.pickaxe.filter((r) => r.owned).length })}
          rows={view.pickaxe}
          nested={PICKAXE_VOCAB}
        />
      )}

      {view?.state === "ok" && view.skins.length > 0 && (
        <h2 className="page-section" data-section-head="skins">
          {t("equip.skinsTitle")}
        </h2>
      )}

      {view?.state === "ok" && view.skins.length > 0 && (
        <NameSection
          title={t("equip.skinsTitle")}
          ownedLabel={t("equip.skinsOwned", { n: view.skins.filter((r) => r.owned).length })}
          rows={view.skins}
        />
      )}

      {open && (
        <>
          <div className="preview-mask" onClick={() => setOpen(null)} />
          <aside className="mod-preview equip-drawer" role="dialog" aria-label={open.name}>
            <button
              className="btn ghost small preview-close"
              onClick={() => setOpen(null)}
              aria-label={t("equip.close")}
            >
              ×
            </button>
            <h2 style={{ marginTop: 0 }}>{open.name}</h2>
            {open.name_en && <p className="dim small">{open.name_en}</p>}
            <dl className="equip-facts">
              <dt>{t("equip.factSlot")}</dt>
              <dd>{t(SLOT_TEXT[open.key] ?? "equip.slotOther")}</dd>
              <dt>{t("equip.factClass")}</dt>
              <dd>
                {classLabel(open.class) ?? t("equip.factClassNone")}
              </dd>
              <dt>{t("equip.factId")}</dt>
              <dd>
                <button
                  className="mono equip-id"
                  title={t("equip.copyId")}
                  onClick={() => {
                    void navigator.clipboard?.writeText(open.id);
                  }}
                >
                  {open.id}
                </button>
              </dd>
            </dl>
            {!open.known && <p className="dim small">{t("equip.unknownName")}</p>}
            {open.oc_total > 0 && (
              <div className="equip-overclock" data-oc-total={open.oc_total}>
                <h3 className="equip-upgrades-title">{t("equip.overclockTitle")}</h3>
                <p className="equip-oc-line">
                  {open.overclock ? (
                    <>
                      <span className="equip-oc-name">{open.overclock.name}</span>
                      {open.overclock.rarity && (
                        <span className="equip-oc-rarity" data-rarity={open.overclock.rarity}>
                          {open.overclock.rarity}
                        </span>
                      )}
                    </>
                  ) : (
                    <span className="dim">{t("equip.ocNone")}</span>
                  )}
                  <span className="dim small">{t("equip.ocSwapCount", { n: open.oc_total })}</span>
                </p>
                <p className="dim small">
                  {open.oc_open ? t("equip.ocOpen") : t("equip.ocLocked")}
                </p>
              </div>
            )}
            {open.upgrades.length > 0 && (
              <div className="equip-upgrades">
                <h3 className="equip-upgrades-title">
                  {t("equip.upgradesTitle", { n: open.upgrades.length })}
                </h3>
                <ul>
                  {open.upgrades.map((u) => (
                    <li
                      key={u.id}
                      className={u.owned ? "equip-upg owned" : "equip-upg"}
                      data-upgrade={u.id}
                    >
                      <span className="equip-upg-mark" aria-hidden="true">
                        {u.equipped ? "●" : u.owned ? "✓" : "·"}
                      </span>
                      <span className="equip-upg-name">{u.name}</span>
                      <span className="equip-upg-state">
                        {!u.in_table
                          ? t("equip.upgMissing")
                          : u.equipped
                            ? t("equip.upgEquipped")
                            : u.owned
                              ? t("equip.upgOwned")
                              : t("equip.upgLocked")}
                      </span>
                    </li>
                  ))}
                </ul>
                {view?.upgrade_note && <p className="dim small">{view.upgrade_note}</p>}
              </div>
            )}
          </aside>
        </>
      )}
    </>
  );
}
