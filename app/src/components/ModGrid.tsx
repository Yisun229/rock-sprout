// mod 列表。用虚拟滚动：只渲染看得见的那几条，几百上千条也不卡。
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
  /** 新的一页结果刚出来时让前几条淡入（行本身用 translateY 定位，动画只能碰透明度） */
  animate?: boolean;
};

/** 只给前 10 条错峰，后面的同时进——否则一千行的列表要排十几秒才露完 */
const STAGGER_MAX = 10;

export default function ModGrid({
  items,
  installing,
  onInstall,
  selected,
  onToggle,
  animate = false,
}: Props) {
  const list = useVirtualList(items.length, ROW_HEIGHT);

  return (
    <div className="vlist" ref={list.scrollRef}>
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
                installing={installing === mod.id}
                disabled={installing !== null}
                onInstall={() => onInstall(mod)}
                selected={selected.includes(mod.id)}
                onToggle={(shift) => onToggle(mod.id, shift)}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}
