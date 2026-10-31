// 矿脉页的三个下拉：类型（多选）、版本、排序。
//
// 类型只能用服务端真实存在的 9 个名字：发出去的是英文原名，界面上显示中文。
// 多选是「同时满足」（AND）不是并集——勾得越多结果越少，所以面板底部固定写一行，
// 勾了两个以上再把当前命中条数摆出来，免得用户以为筛选坏了。
import { useEffect, useRef, useState } from "react";
import { t, thousands } from "../i18n";
import type { Filters, SortKey } from "../store/appStore";
import { TYPE_TAGS } from "../tags";

/** 可以筛的版本。mod.io 没有单独的「版本列表」接口，版本本身就是标签，这里按实测写死。 */
const VERSIONS = ["1.40", "1.39", "1.38", "1.37", "1.36"];

const SORTS: [SortKey, string][] = [
  ["downloads", "下载量最多"],
  ["latest", "最新发布"],
];

type Open = "type" | "version" | "sort" | null;

type Props = {
  filters: Filters;
  onChange: (next: Filters) => void;
  /** 当前筛选下命中多少条；还没查完就是 null */
  total: number | null;
  busy: boolean;
};

export default function Filters({ filters, onChange, total, busy }: Props) {
  const [open, setOpen] = useState<Open>(null);
  const root = useRef<HTMLDivElement | null>(null);

  // 点别处、按 Esc 都收起来（不然三个面板会叠着开）
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) setOpen(null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(null);
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const toggleTag = (tag: string) => {
    const tags = filters.tags.includes(tag)
      ? filters.tags.filter((x) => x !== tag)
      : [...filters.tags, tag];
    onChange({ ...filters, tags });
  };

  const typeLabel =
    filters.tags.length === 0
      ? t("filter.typeAll")
      : filters.tags.length === 1
        ? (TYPE_TAGS.find(([v]) => v === filters.tags[0])?.[1] ?? filters.tags[0])
        : t("filter.typeCount", { count: filters.tags.length });

  const sortLabel = SORTS.find(([v]) => v === filters.sort)?.[1] ?? SORTS[0][1];

  return (
    <div className="filters" ref={root}>
      {/* 类型：多选 */}
      <div className="filter">
        <button
          className={open === "type" ? "btn ghost small on" : "btn ghost small"}
          aria-expanded={open === "type"}
          onClick={() => setOpen(open === "type" ? null : "type")}
        >
          {t("filter.type")}：{typeLabel}
        </button>
        {open === "type" && (
          <div className="filter-panel" data-testid="filter-type">
            <div className="filter-list">
              {TYPE_TAGS.map(([value, label]) => (
                <label className="filter-item" key={value}>
                  <input
                    type="checkbox"
                    className="pick"
                    checked={filters.tags.includes(value)}
                    onChange={() => toggleTag(value)}
                  />
                  <span>{label}</span>
                </label>
              ))}
            </div>
            <div className="filter-foot">
              <span>{t("filter.andHint")}</span>
              {filters.tags.length >= 2 && (
                <span className="filter-count">
                  {busy || total === null
                    ? t("filter.countPending")
                    : t("filter.countNow", { count: thousands(total) })}
                </span>
              )}
            </div>
          </div>
        )}
      </div>

      {/* 版本：单选 */}
      <div className="filter">
        <button
          className={open === "version" ? "btn ghost small on" : "btn ghost small"}
          aria-expanded={open === "version"}
          onClick={() => setOpen(open === "version" ? null : "version")}
        >
          {t("filter.version")}：{filters.version ?? t("filter.versionAll")}
        </button>
        {open === "version" && (
          <div className="filter-panel" data-testid="filter-version">
            <div className="filter-list">
              <label className="filter-item">
                <input
                  type="radio"
                  className="pick"
                  name="version"
                  checked={filters.version === null}
                  onChange={() => onChange({ ...filters, version: null })}
                />
                <span>{t("filter.versionAll")}</span>
              </label>
              {VERSIONS.map((v) => (
                <label className="filter-item" key={v}>
                  <input
                    type="radio"
                    className="pick"
                    name="version"
                    checked={filters.version === v}
                    onChange={() => onChange({ ...filters, version: v })}
                  />
                  <span>{v}</span>
                </label>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* 排序：单选 */}
      <div className="filter">
        <button
          className={open === "sort" ? "btn ghost small on" : "btn ghost small"}
          aria-expanded={open === "sort"}
          onClick={() => setOpen(open === "sort" ? null : "sort")}
        >
          {t("filter.sort")}：{sortLabel}
        </button>
        {open === "sort" && (
          <div className="filter-panel" data-testid="filter-sort">
            <div className="filter-list">
              {SORTS.map(([value, label]) => (
                <label className="filter-item" key={value}>
                  <input
                    type="radio"
                    className="pick"
                    name="sort"
                    checked={filters.sort === value}
                    onChange={() => onChange({ ...filters, sort: value })}
                  />
                  <span>{label}</span>
                </label>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
