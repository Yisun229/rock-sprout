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
import type { Filters } from "../store/appStore";
import { useAppStore } from "../store/appStore";
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
  // 筛选条件放全局：切页再回来还在，抽屉开着也不影响
  const { filters } = useAppStore();
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
  /** 防抖回调里要读最新的输入框内容，不能把它写进依赖（依赖一变就又搜一次） */
  const draftRef = useRef(draft);
  draftRef.current = draft;
  /** 首次渲染不因为「筛选初始值」多搜一次 */
  const firstRender = useRef(true);
  /** 自动搜那一处只关心关键词变化，下拉由下面的 effect 负责；用 ref 读最新值避免多搜一次 */
  const filtersRef = useRef(filters);
  filtersRef.current = filters;

  const doSearch = useCallback(async (query: string, nextPage: number, by: Filters) => {
    setBusy(true);
    setError(null);
    setNote("");
    // 没有任何筛选时走 search_mods：那条路会写缓存，断网时能回放上次结果
    const plain = by.tags.length === 0 && by.version === null && by.sort === "downloads";
    try {
      setPage(
        plain
          ? await call<SearchPage>("search_mods", { query, page: nextPage })
          : await call<SearchPage>("list_mods", {
              keyword: query.trim() ? query.trim() : null,
              tags: by.tags,
              version: by.version,
              sort: by.sort,
              page: nextPage,
            }),
      );
    } catch (e) {
      setPage(null);
      setError(toApiError(e));
      setRetry(() => () => void doSearch(query, nextPage, by));
    } finally {
      setBusy(false);
    }
  }, []);

  // 输入停下来就自动搜一次（空关键词不自动搜：想看热门就点「搜索」）。
  useEffect(() => {
    const query = settled.trim();
    if (query === "" || query === searched.current || busy) return;
    searched.current = query;
    void doSearch(query, 1, filtersRef.current);
  }, [settled, busy, doSearch]);

  // 三个下拉一动就重新查第一页；沿用同一个 300ms 防抖，连点两下只发一次
  useEffect(() => {
    if (firstRender.current) {
      firstRender.current = false;
      return;
    }
    const timer = setTimeout(() => {
      const query = draftRef.current.trim();
      searched.current = query;
      void doSearch(query, 1, filters);
    }, 300);
    return () => clearTimeout(timer);
  }, [filters, doSearch]);

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
    void doSearch(draft, 1, filters);
  }, [draft, doSearch, filters]);

  const goPage = useCallback(
    (next: number) => void doSearch(page?.query ?? draft, next, filters),
    [draft, doSearch, filters, page?.query],
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
