// 装备页的「博斯科」块：改装（可折叠）+ 涂装（一行）。
//
// 只读。改装分母是 11（13 条资产里 2 条从未在存档中出现，标 unused 不显示）；
// 涂装分母是 45（`OwningCharacter == BoscoID` 的皮肤资产），不是 `Drone/Skins` 目录的 39。
import { useCallback, useEffect, useState } from "react";
import { call, type BoscoView } from "../../api";
import { t, thousands } from "../../i18n";
import { costOf, effectOf } from "../../upgradeText";
import { useAppStore } from "../../store/appStore";

function dupNames(rows: BoscoView["upgrades"]): Set<string> {
  const seen = new Map<string, number>();
  for (const r of rows) seen.set(r.name, (seen.get(r.name) ?? 0) + 1);
  return new Set([...seen.entries()].filter(([, n]) => n > 1).map(([k]) => k));
}

export default function BoscoBlock() {
  const { page } = useAppStore();
  const [view, setView] = useState<BoscoView | null>(null);

  const load = useCallback(async () => {
    try {
      setView(await call<BoscoView>("bosco_view", {}));
    } catch {
      setView(null);
    }
  }, []);

  useEffect(() => {
    if (page !== "equip") return;
    void load();
  }, [page, load]);

  if (!view || view.state !== "ok") {
    return (
      <div className="card" data-block="bosco">
        <div className="card-title-row">
          <span className="card-title">{t("equip.boscoTitle")}</span>
        </div>
        <p className="dim small">{view?.note || t("equip.boscoNoData")}</p>
      </div>
    );
  }

  const dups = dupNames(view.upgrades);
  const equipped = view.upgrades.filter((u) => u.equipped);
  const skins = view.currentSkin
    .map((s) => s.name?.zh || s.name?.en || s.id.slice(0, 8))
    .filter(Boolean);

  return (
    <div className="card" data-block="bosco" data-bosco="1">
      <div className="card-title-row">
        <span className="card-title">{t("equip.boscoTitle")}</span>
        <span className="dim small" data-bosco-count="1">
          {t("equip.boscoUpgradesCount", {
            owned: view.upgradesOwned,
            total: view.upgradesTotal,
          })}
        </span>
      </div>

      <details className="bosco-fold" data-fold="upgrades" data-count={view.upgrades.length}>
        <summary className="bosco-fold-sum">
          <span>{t("equip.boscoUpgrades")}</span>
          <span className="dim small">
            {equipped.length > 0
              ? t("equip.boscoEquippedCount", { n: equipped.length })
              : t("equip.boscoUpgradesCount", { owned: view.upgradesOwned, total: view.upgradesTotal })}
          </span>
        </summary>
        <ul className="bosco-upgrades">
          {[...view.upgrades]
            .sort((a, b) => Number(b.equipped) - Number(a.equipped) || Number(b.owned) - Number(a.owned))
            .map((u) => (
            <li
              key={u.id}
              className={u.owned ? "bosco-upg owned" : "bosco-upg"}
              data-upgrade={u.id}
              data-owned={u.owned ? "1" : "0"}
              data-equipped={u.equipped ? "1" : "0"}
            >
              <span className="equip-upg-mark" aria-hidden="true">
                {u.equipped ? "●" : u.owned ? "✓" : "·"}
              </span>
              <span className="bosco-upg-name">
                {u.name}
                {dups.has(u.name) && <span className="bosco-dup">{u.asset}</span>}
              </span>
              <span className="bosco-upg-state">
                {u.equipped
                  ? t("equip.boscoEquipped")
                  : u.owned
                    ? t("equip.boscoOwned")
                    : t("equip.boscoLocked")}
              </span>
              {!u.owned && (
                <>
                  <span className="bosco-upg-extra" data-bosco-effect="1">
                    {effectOf(u.id) ?? t("equip.upgNoEffect")}
                  </span>
                  <span className="bosco-upg-extra bosco-upg-cost" data-bosco-cost="1">
                    {(() => {
                      const cost = costOf(u.id);
                      const parts = [
                        cost.credits > 0
                          ? t("equip.upgCredits", { n: thousands(cost.credits) })
                          : null,
                        ...cost.resources,
                      ].filter(Boolean);
                      if (parts.length === 0) return t("equip.upgNoCost");
                      return `${t("equip.upgUnlock")} ${parts.join(" · ")}`;
                    })()}
                  </span>
                </>
              )}
            </li>
            ))}
        </ul>
      </details>

      <p className="bosco-skins" data-skins={`${view.skinsOwned}/${view.skinsTotal}`}>
        <span>
          {t("equip.boscoCurrentSkin")}
          <span className="bosco-skin-names">{skins.length > 0 ? skins.join(" / ") : t("equip.boscoNoSkin")}</span>
          <span className="dim small">
            {" "}
            {skins.length === 2
              ? t("equip.boscoSkinNote2")
              : t("equip.boscoSkinNoteN", { n: skins.length })}
          </span>
        </span>
        <span className="dim small">
          {t("equip.boscoSkinsCount", { owned: view.skinsOwned, total: view.skinsTotal })}
        </span>
      </p>
    </div>
  );
}
