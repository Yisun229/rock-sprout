// 皮肤 / 十字镐这类「名字清单」：首屏只列存档里有的，点开关再展开全量。
//
// 从 `pages/Equipment.tsx` 里抽出来的，抽的时候只做了一件事：把原来写死的
// 十字镐词表（刀刃/手柄/其他、前刃…尾椎）换成参数，行为一个字没改——
// `data-section`/`data-group`/`data-part`/`name-cell` 这些钩子保持原样，
// 回归里那几条断言依赖它们。
import { useState } from "react";
import type { OwnedNameRow } from "../api";

/// 中文名在前、英文名在后；只用来排序，不判断某条是不是「有中文」
function hasCjk(text: string) {
  return /[\u3400-\u9fff]/.test(text);
}

/** 按「属于哪个武器」分组；组内中文名在前、英文名在后 */
function groupByHint(rows: OwnedNameRow[]): [string, OwnedNameRow[]][] {
  const groups = new Map<string, OwnedNameRow[]>();
  for (const row of rows) {
    const hint = row.hint || "未标注归属";
    if (!groups.has(hint)) groups.set(hint, []);
    groups.get(hint)!.push(row);
  }
  return [...groups.entries()]
    .sort((a, b) => a[0].localeCompare(b[0], "zh-Hans-CN"))
    .map(([hint, items]) => [
      hint,
      items.slice().sort((a, b) => {
        const ca = hasCjk(a.name) ? 0 : 1;
        const cb = hasCjk(b.name) ? 0 : 1;
        return ca - cb || a.name.localeCompare(b.name, "zh-Hans-CN");
      }),
    ]);
}

function orderBy<T>(items: T[], key: (x: T) => string, order: string[]): T[] {
  const rank = (v: string) => {
    const i = order.indexOf(v);
    return i < 0 ? order.length : i;
  };
  return items
    .slice()
    .sort((a, b) => rank(key(a)) - rank(key(b)) || key(a).localeCompare(key(b), "zh-Hans-CN"));
}

/** 两层分组用的词表（十字镐传它自己的；别的清单不传就是单层） */
export type NestedVocab = {
  /** 大组顺序：刀刃 / 手柄 / 其他 */
  groups: string[];
  /** 组内部位顺序：前刃 … 尾椎 */
  parts: string[];
};

export function NameSection({
  title,
  ownedLabel,
  rows,
  nested,
}: {
  title: string;
  ownedLabel: string;
  rows: OwnedNameRow[];
  /** 十字镐：先按大组折叠，再按具体部位分组 */
  nested?: NestedVocab;
}) {
  const [showAll, setShowAll] = useState(false);

  const list = showAll ? rows : rows.filter((r) => r.owned);
  const groups = groupByHint(list);
  const ordered = nested ? orderBy(groups, ([hint]) => hint, nested.groups) : groups;

  const cell = (r: OwnedNameRow) => (
    <span
      className={r.owned ? "name-cell" : "name-cell name-cell-off"}
      key={r.id}
      data-owned={r.owned ? "1" : "0"}
    >
      {r.name || r.id.slice(0, 8)}
    </span>
  );

  return (
    <div className="card" data-section={title}>
      <div className="card-title-row">
        <span className="card-title">{title}</span>
        <span className="dim small">{ownedLabel}</span>
        <button
          className="btn small"
          style={{ marginLeft: "auto" }}
          onClick={() => setShowAll((v) => !v)}
        >
          {showAll ? "只看有的" : "显示全部"}
        </button>
      </div>
      <div className="name-grid name-groups">
        {ordered.map(([hint, items]) => (
          <div className="name-group" key={hint} data-group={hint}>
            {nested ? (
              <details className="name-group-fold" open>
                <summary className="name-group-title">
                  <span>{hint}</span>
                  <span className="name-group-count">{items.length} 件</span>
                </summary>
                {orderBy(
                  [...items
                    .reduce((m, r) => {
                      const k = r.part || "其他";
                      (m.get(k) ?? m.set(k, []).get(k)!).push(r);
                      return m;
                    }, new Map<string, OwnedNameRow[]>())
                    .entries()],
                  ([part]) => part,
                  nested.parts,
                ).map(([part, cells]) => (
                  <div className="name-sub" key={part} data-part={part}>
                    <div className="name-sub-title">{part}</div>
                    <div className="name-grid">{cells.map(cell)}</div>
                  </div>
                ))}
              </details>
            ) : (
              <>
                <div className="name-group-title">{hint}</div>
                <div className="name-grid">{items.map(cell)}</div>
              </>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
