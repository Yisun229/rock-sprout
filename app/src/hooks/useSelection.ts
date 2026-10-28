// 多选：点一下选中 / 再点取消 / Shift 连选一段 / Ctrl+A 全选 / Esc 清空。
//
// 现在还没接界面（M2 的批量安装要用），但先把规则写在一处，免得散到各页面里。
import { useCallback } from "react";

export type Selection = {
  ids: number[];
  has: (id: number) => boolean;
  /** 单击：已选就取消，没选就加进去（`anchor` 用来给 Shift 连选当起点） */
  toggle: (id: number) => void;
  /** Shift 连选：把 anchor 到 id 之间的一段（按传入的顺序）都选上 */
  selectRange: (order: number[], anchor: number, id: number) => void;
  selectAll: (order: number[]) => void;
  clear: () => void;
};

export function useSelection(
  ids: number[],
  setIds: (next: number[]) => void,
): Selection {
  const has = useCallback((id: number) => ids.includes(id), [ids]);

  const toggle = useCallback(
    (id: number) => setIds(ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]),
    [ids, setIds],
  );

  const selectRange = useCallback(
    (order: number[], anchor: number, id: number) => {
      const from = order.indexOf(anchor);
      const to = order.indexOf(id);
      if (from < 0 || to < 0) return setIds(ids.includes(id) ? ids : [...ids, id]);
      const [lo, hi] = from <= to ? [from, to] : [to, from];
      const span = order.slice(lo, hi + 1);
      setIds([...new Set([...ids, ...span])]);
    },
    [ids, setIds],
  );

  const selectAll = useCallback((order: number[]) => setIds(order), [setIds]);
  const clear = useCallback(() => setIds([]), [setIds]);

  return { ids, has, toggle, selectRange, selectAll, clear };
}
