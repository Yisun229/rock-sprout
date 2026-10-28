// mod 列表。用虚拟滚动：只渲染看得见的那几条，几百上千条也不卡。
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
};

export default function ModGrid({ items, installing, onInstall, selected, onToggle }: Props) {
  const list = useVirtualList(items.length, ROW_HEIGHT);

  return (
    <div className="vlist" ref={list.scrollRef}>
      <div className="vlist-inner" style={{ height: list.totalSize }}>
        {list.items.map((slot) => {
          const mod = items[slot.index];
          if (!mod) return null;
          return (
            <div
              key={mod.id}
              className="vrow"
              style={{ transform: `translateY(${slot.start}px)`, height: slot.size }}
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
