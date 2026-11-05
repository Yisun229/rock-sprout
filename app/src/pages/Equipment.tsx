// 「装备」：每个角色槽三套配装，每套八个格子。
//
// 只读——这一页永远不改存档。名字来自 `app/src/data/weapon-names.json`，
// 查不到的格子就显示 `#编号前 8 位`，不猜。
import { useCallback, useEffect, useState } from "react";
import { call, type CharacterEquip, type EquipmentItem, type EquipmentView } from "../api";
import { t } from "../i18n";
import { useAppStore } from "../store/appStore";

/** i18n 的键类型（避免把随便一个字符串当键传进来） */
type MsgKey = Parameters<typeof t>[0];

/** 存档里记的职业代号 → 界面上的中文；认不出用 equip.unknownClass */
const CLASS_TEXT: Record<string, MsgKey> = {
  driller: "equip.classDriller",
  engineer: "equip.classEngineer",
  gunner: "equip.classGunner",
  scout: "equip.classScout",
};

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
  if (one.class && CLASS_TEXT[one.class]) return t(CLASS_TEXT[one.class]);
  return t("equip.unknownClass", { slot: one.slot });
}

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

      {busy && !view && <p className="dim small">{t("equip.loading")}</p>}

      {bad && view && (
        <div className="card">
          <p style={{ margin: 0 }}>{view.note || t("equip.notFound")}</p>
          <button className="btn small" style={{ marginTop: 10 }} onClick={() => void load()}>
            {t("equip.retry")}
          </button>
        </div>
      )}

      {view?.state === "ok" &&
        view.characters.map((one) => (
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
                {open.class && CLASS_TEXT[open.class]
                  ? t(CLASS_TEXT[open.class])
                  : t("equip.factClassNone")}
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
