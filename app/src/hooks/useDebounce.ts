// 搜索框防抖：输入停下来 350ms 才把值交出去，免得每敲一个字都打一次后端。
import { useEffect, useState } from "react";

export function useDebounce<T>(value: T, delayMs = 350): T {
  const [settled, setSettled] = useState(value);

  useEffect(() => {
    const id = setTimeout(() => setSettled(value), delayMs);
    return () => clearTimeout(id);
  }, [value, delayMs]);

  return settled;
}
