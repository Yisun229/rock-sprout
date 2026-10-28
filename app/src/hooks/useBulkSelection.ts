// 一个列表页上的多选交互：单击可选、Shift 连选一段、Ctrl+A 全选、Esc 清空。
//
// 选中集合放在调用方（背包放全局 store，矿脉放本页），这里只管规则与键盘。
import { useEffect, useRef } from "react";
import { useSelection, type Selection } from "./useSelection";

export type BulkSelection = Selection & {
  /** 点一行：`shift` 为真时从上次点的那一行连选过来 */
  toggleWithShift: (id: number, shift: boolean) => void;
};

/**
 * @param order 当前列表的 id 顺序（Shift 连选要看顺序）
 * @param active 这一页是不是正显示着（藏起来的页不该抢键盘）
 */
export function useBulkSelection(
  order: number[],
  ids: number[],
  setIds: (next: number[]) => void,
  active: boolean,
): BulkSelection {
  const sel = useSelection(ids, setIds);
  const anchor = useRef<number | null>(null);
  const orderRef = useRef(order);
  orderRef.current = order;

  const toggleWithShift = (id: number, shift: boolean) => {
    if (shift && anchor.current !== null) {
      sel.selectRange(orderRef.current, anchor.current, id);
    } else {
      sel.toggle(id);
    }
    anchor.current = id;
  };

  const { selectAll, clear } = sel;
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      const typing =
        e.target instanceof HTMLElement &&
        (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA");
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
        if (typing) return;
        e.preventDefault();
        selectAll(orderRef.current);
      } else if (e.key === "Escape") {
        clear();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, selectAll, clear]);

  return { ...sel, toggleWithShift };
}
