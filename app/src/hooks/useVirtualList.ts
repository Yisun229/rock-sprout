// 长列表只渲染看得见的那几条。1000 条结果时 DOM 里始终只有十几个节点，滚动不会卡。
//
// 固定行高（`estimateSize` 给死值）是故意的：行高一致时不用测量，滚动最稳，
// 也不会因为卡片里文字换行导致列表跳动。
import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

export type VirtualList = {
  /** 挂到滚动容器上 */
  scrollRef: (node: HTMLDivElement | null) => void;
  /** 给内层占位用的总高度 */
  totalSize: number;
  /** 当前该渲染的几条（带绝对位置） */
  items: { index: number; start: number; size: number }[];
};

export function useVirtualList(
  count: number,
  rowHeight: number,
  overscan = 4,
): VirtualList {
  const parentRef = useRef<HTMLDivElement | null>(null);

  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeight,
    overscan,
  });

  return {
    scrollRef: (node) => {
      parentRef.current = node;
    },
    totalSize: virtualizer.getTotalSize(),
    items: virtualizer.getVirtualItems().map((v) => ({
      index: v.index,
      start: v.start,
      size: v.size,
    })),
  };
}
