// 翻译：行内「译」翻简介，底部栏「翻译选中」批量翻。
//
// 后端一次翻一段文字，批量这边自己排队：最多 10 个、同时 2 个、每发一次隔 400ms，
// 太密会撞上接口的限流（429），那不是用户该操心的事。
import { useCallback, useRef, useState } from "react";
import { call, toApiError } from "../api";

/** 一次最多翻多少个 */
export const BATCH_LIMIT = 10;
/** 同时进行几个 */
const CONCURRENCY = 2;
/** 两次请求之间至少隔多久 */
const GAP_MS = 400;

export type RowTranslation = {
  /** 译好的中文 */
  text: string;
  /** 失败时的原因（给人看的） */
  error: string | null;
};

export type TranslateStore = {
  /** 已经翻好的行：mod id → 译文 */
  rows: Record<number, RowTranslation>;
  /** 正在翻的行 */
  busy: Set<number>;
  /** 批量翻译的进度；没在跑就是 null */
  progress: { done: number; total: number } | null;
};

// 注意：`translate_text` 只回一段字符串，`translate_detail` 才回 { text, cached }

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** 把后端错误翻成一句人话 */
function reason(e: unknown): string {
  const err = toApiError(e);
  if (err.code === "unauthorized") return "令牌失效了，去设置页重贴一个";
  if (err.code === "rate_limited") return "翻得太快被限流了，歇一会儿再试";
  if (err.code === "network") return "连不上翻译服务";
  if (err.code === "too_long") return "这段太长，翻不了";
  return err.message || "翻译失败";
}

export function useTranslate() {
  const [store, setStore] = useState<TranslateStore>({
    rows: {},
    busy: new Set(),
    progress: null,
  });
  /** 换页 / 离开时把还在跑的批量停掉 */
  const stopped = useRef(false);

  const setBusyRow = useCallback((id: number, on: boolean) => {
    setStore((prev) => {
      const busy = new Set(prev.busy);
      if (on) busy.add(id);
      else busy.delete(id);
      return { ...prev, busy };
    });
  }, []);

  /** 翻一段文字并落到某一行上；返回成功与否 */
  const runOne = useCallback(
    async (id: number, text: string) => {
      setBusyRow(id, true);
      try {
        // target 传空串 = 用设置里存的默认语言
        const out = await call<string>("translate_text", { text, target: "" });
        setStore((prev) => ({
          ...prev,
          rows: { ...prev.rows, [id]: { text: out, error: null } },
        }));
        return true;
      } catch (e) {
        setStore((prev) => ({
          ...prev,
          rows: { ...prev.rows, [id]: { text: "", error: reason(e) } },
        }));
        return false;
      } finally {
        setBusyRow(id, false);
      }
    },
    [setBusyRow],
  );

  /** 点一次「译」：没译就译，译过就收起（回到原文） */
  const toggleOne = useCallback(
    async (id: number, text: string) => {
      const done = store.rows[id];
      if (done && !done.error) {
        setStore((prev) => {
          const rows = { ...prev.rows };
          delete rows[id];
          return { ...prev, rows };
        });
        return;
      }
      await runOne(id, text || "");
    },
    [runOne, store.rows],
  );

  /** 底部栏「翻译选中」：最多 10 个、同时 2 个、间隔 400ms */
  const translateMany = useCallback(
    async (mods: { id: number; summary: string }[]) => {
      const todo = mods
        .filter((m) => !store.rows[m.id] || store.rows[m.id].error)
        .slice(0, BATCH_LIMIT);
      if (todo.length === 0) return;
      stopped.current = false;
      setStore((prev) => ({ ...prev, progress: { done: 0, total: todo.length } }));

      let cursor = 0;
      let done = 0;
      const worker = async () => {
        while (!stopped.current) {
          const index = cursor++;
          if (index >= todo.length) return;
          const mod = todo[index];
          await runOne(mod.id, mod.summary || "");
          if (stopped.current) return;
          done += 1;
          setStore((prev) => ({ ...prev, progress: { done, total: todo.length } }));
          // 两个 worker 各自隔 400ms，合起来大约每秒 4 个请求
          await sleep(GAP_MS);
        }
      };
      await Promise.all(Array.from({ length: Math.min(CONCURRENCY, todo.length) }, worker));
      setStore((prev) => ({ ...prev, progress: null }));
    },
    [runOne, store.rows],
  );

  const stop = useCallback(() => {
    stopped.current = true;
    setStore((prev) => ({ ...prev, progress: null }));
  }, []);

  return { ...store, toggleOne, translateMany, stop };
}
