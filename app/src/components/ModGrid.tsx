// mod 列表。用虚拟滚动：只渲染看得见的那几条，几百上千条也不卡。
//
// 键盘走法（M8-3）：整张列表只有一个 Tab 停靠点——当前行。
// ↑/↓ 换行，Enter 打开抽屉，Esc 退回搜索框。行本身不给每个都加 tabIndex，
// 只有「当前行」是 0，其余都是 -1（roving tabindex），不然一千行就是一千次 Tab。
import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import type { ModSummary } from "../api";
import { useVirtualList } from "../hooks/useVirtualList";
import ModCard from "./ModCard";

/** 行高给死值（跟样式里的 .vrow 一致），虚拟滚动才不用测量。 */
const ROW_HEIGHT = 72;

type Props = {
  items: ModSummary[];
  installing: number | null;
  onInstall: (mod: ModSummary) => void;
  /** 勾选状态：`onToggle` 的 `shift` 为真表示要连选一段 */
  selected: number[];
  onToggle: (id: number, shift: boolean) => void;
  /** 点一行 → 右边开预览（再点同一行由上层负责关掉） */
  onOpen: (id: number) => void;
  /** 预览正开着哪个 mod */
  previewId: number | null;
  /** 新的一页结果刚出来时让前几条淡入（行本身用 translateY 定位，动画只能碰透明度） */
  animate?: boolean;
  /** 已经翻好的简介：mod id → 中文 */
  translated?: Record<number, { text: string; error: string | null }>;
  /** 正在翻的那几条 */
  translating?: Set<number>;
  /** 点「译」 */
  onTranslate?: (mod: ModSummary) => void;
};

/** 只给前 10 条错峰，后面的同时进——否则一千行的列表要排十几秒才露完 */
const STAGGER_MAX = 10;

export default function ModGrid({
  items,
  installing,
  onInstall,
  selected,
  onToggle,
  onOpen,
  previewId,
  animate = false,
  translated = {},
  translating,
  onTranslate,
}: Props) {
  const list = useVirtualList(items.length, ROW_HEIGHT);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  /** 当前行：整张列表里唯一可被 Tab 停住的那一行 */
  const [active, setActive] = useState(0);

  // 换了一批结果（翻页、换筛选、重搜），当前行回到第一条
  useEffect(() => {
    setActive(0);
  }, [items]);

  /** 换行：先滚到位置，等浏览器把这一行画出来再给焦点 */
  const moveTo = (index: number) => {
    const next = Math.max(0, Math.min(index, items.length - 1));
    setActive(next);
    if (scrollRef.current) scrollRef.current.scrollTop = next * ROW_HEIGHT;
    requestAnimationFrame(() => {
      const el = scrollRef.current?.querySelector<HTMLElement>(`.row[data-idx="${next}"]`);
      el?.focus({ preventScroll: true });
    });
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (items.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveTo(active + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      moveTo(active - 1);
    } else if (e.key === "Enter") {
      // 焦点在「译」「装」上时它们自己处理，别把抽屉也一起打开
      const target = e.target as HTMLElement;
      if (target.classList.contains("row")) {
        e.preventDefault();
        const mod = items[active];
        if (mod) onOpen(mod.id);
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      const input = document.querySelector<HTMLInputElement>(".searchbar input");
      input?.focus();
    }
  };

  return (
    <div
      className="vlist"
      role="listbox"
      aria-label="mod 列表"
      onKeyDown={onKeyDown}
      ref={(node) => {
        list.scrollRef(node);
        scrollRef.current = node;
      }}
    >
      <div className="vlist-inner" style={{ height: list.totalSize }}>
        {list.items.map((slot) => {
          const mod = items[slot.index];
          if (!mod) return null;
          const staggered = animate && slot.index < STAGGER_MAX;
          return (
            <div
              key={mod.id}
              className={staggered ? "vrow anim-row" : "vrow"}
              style={
                {
                  transform: `translateY(${slot.start}px)`,
                  height: slot.size,
                  ...(staggered ? { "--i": slot.index } : {}),
                } as CSSProperties
              }
            >
              <ModCard
                mod={mod}
                idx={slot.index}
                active={slot.index === active}
                installing={installing === mod.id}
                disabled={installing !== null}
                onInstall={() => onInstall(mod)}
                selected={selected.includes(mod.id)}
                onToggle={(shift) => onToggle(mod.id, shift)}
                onOpen={() => onOpen(mod.id)}
                opened={previewId === mod.id}
                translated={translated[mod.id]?.text ?? null}
                translating={translating?.has(mod.id) ?? false}
                translateError={translated[mod.id]?.error ?? null}
                onTranslate={onTranslate ? () => onTranslate(mod) : undefined}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}
