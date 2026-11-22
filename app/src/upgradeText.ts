// 改装文字的查表小工具：装备页的武器改装和博斯科改装共用。
//
// 两张表都由 `_tools/_buildUpgradeTexts.mjs` / `_tools/_buildUpgradeCosts.mjs`
// 从游戏资产抽出来，键是改装的 SaveGameID（小写）。
import upgradeCosts from "./data/upgrade-costs.json";
import upgradeTexts from "./data/upgrade-texts.json";

type UpgradeTexts = {
  texts: Record<
    string,
    {
      asset: string;
      name: { en: string; zh: string | null } | null;
      desc: { en: string; zh: string | null } | null;
    }
  >;
};

type UpgradeCosts = {
  costs: Record<
    string,
    {
      asset: string;
      credits: number;
      resources: { asset: string; zh: string | null; amount: number | null }[];
    }
  >;
};

/** 未解锁改装的一句话效果（中文优先，没有就英文原文）。 */
export function effectOf(id: string): string | null {
  const one = (upgradeTexts as UpgradeTexts).texts[id];
  const text = one?.desc?.zh || one?.desc?.en || null;
  return text ? text.replace(/\r\n/g, " ").trim() : null;
}

/** 未解锁改装的解锁消耗：信用点 + 矿（数量未知时只报矿名）。 */
export function costOf(id: string): { credits: number; resources: string[] } {
  const one = (upgradeCosts as UpgradeCosts).costs[id];
  if (!one) return { credits: 0, resources: [] };
  return {
    credits: one.credits,
    resources: one.resources.map((r) =>
      r.amount === null ? r.zh || r.asset : `${r.zh || r.asset} ×${r.amount}`,
    ),
  };
}
