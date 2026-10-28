// 矿脉要的数据与动作：搜、翻页、装、出错后重试。
//
// 抽成 hook 是为了让矿脉页只剩排版（也顺便让「先清错误再发请求」这套顺序只写一遍）。
import { useCallback, useEffect, useRef, useState } from "react";
import {
  call,
  toApiError,
  type ApiError,
  type InstallOutcome,
  type ModSummary,
  type SearchPage,
} from "../api";
import { t } from "../i18n";
import { useDebounce } from "./useDebounce";

export type SearchState = {
  draft: string;
  setDraft: (next: string) => void;
  page: SearchPage | null;
  busy: boolean;
  installing: number | null;
  note: string;
  error: ApiError | null;
  retry: () => void;
  /** 用输入框里的字搜第一页（按钮/回车走的这条） */
  searchNow: () => void;
  goPage: (next: number) => void;
  install: (mod: ModSummary) => void;
};

export function useSearch(): SearchState {
  const [draft, setDraft] = useState("");
  const [page, setPage] = useState<SearchPage | null>(null);
  const [busy, setBusy] = useState(false);
  const [installing, setInstalling] = useState<number | null>(null);
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const [retry, setRetry] = useState<() => void>(() => () => {});
  const settled = useDebounce(draft, 350);
  /** 上一次已经搜过的关键词：手动搜过一次，就别再让防抖搜第二遍 */
  const searched = useRef("");

  const doSearch = useCallback(async (query: string, nextPage: number) => {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      setPage(await call<SearchPage>("search_mods", { query, page: nextPage }));
    } catch (e) {
      setPage(null);
      setError(toApiError(e));
      setRetry(() => () => void doSearch(query, nextPage));
    } finally {
      setBusy(false);
    }
  }, []);

  // 输入停下来就自动搜一次（空关键词不自动搜：想看热门就点「搜索」）。
  useEffect(() => {
    const query = settled.trim();
    if (query === "" || query === searched.current || busy) return;
    searched.current = query;
    void doSearch(query, 1);
  }, [settled, busy, doSearch]);

  const doInstall = useCallback(async (mod: ModSummary) => {
    setInstalling(mod.id);
    setError(null);
    setNote(t("mine.installing", { name: mod.name }));
    try {
      const res = await call<InstallOutcome>("install_mod", { id: mod.id });
      setNote(
        res.pak
          ? t("mine.installed", { name: res.name, pak: res.pak })
          : t("mine.installedNoPak", { name: res.name }),
      );
    } catch (e) {
      setNote("");
      setError(toApiError(e));
      setRetry(() => () => void doInstall(mod));
    } finally {
      setInstalling(null);
    }
  }, []);

  const searchNow = useCallback(() => {
    searched.current = draft.trim();
    void doSearch(draft, 1);
  }, [draft, doSearch]);

  const goPage = useCallback(
    (next: number) => void doSearch(page?.query ?? draft, next),
    [draft, doSearch, page?.query],
  );

  return {
    draft,
    setDraft,
    page,
    busy,
    installing,
    note,
    error,
    retry,
    searchNow,
    goPage,
    install: (mod) => void doInstall(mod),
  };
}
