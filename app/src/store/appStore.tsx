// 全局状态：当前在哪一页、已装列表、之后还要放「选中的 mod」。
//
// 用 context + reducer，而不是每页各管各的：切页之后「已装列表」还要在（背包用完矿脉也要用），
// 而且 M2 的多选集合得有一个活过切页的落脚点。
import { createContext, useContext, useMemo, useReducer, type ReactNode } from "react";
import type { InstalledMod } from "../api";

/** 左侧导航的四个页：今天在最上面，其余三个归在「矿层」组里。 */
export type PageKey = "today" | "mine" | "bag" | "settings";

type State = {
  page: PageKey;
  /** 已经进过的页：进过一次就一直挂在 DOM 里（用 CSS 藏起来），状态不会丢 */
  visited: Record<PageKey, boolean>;
  installed: InstalledMod[] | null;
  /** 多选集合（M2 的批量操作用；这里先把位置留出来） */
  selected: number[];
  /** 「粘个令牌」那张引导卡已收起（Q2，存在 ui-state.json 里） */
  hintCollapsed: boolean;
  /** 抽屉预览打开的是哪个 mod；null = 抽屉关着 */
  previewModId: number | null;
};

type Action =
  | { type: "go"; page: PageKey }
  | { type: "installed"; list: InstalledMod[] }
  | { type: "select"; id: number }
  | { type: "setSelection"; ids: number[] }
  | { type: "clearSelection" }
  | { type: "hintCollapsed"; value: boolean }
  | { type: "preview"; id: number | null };

const initial: State = {
  page: "today",
  visited: { today: true, mine: false, bag: false, settings: false },
  installed: null,
  selected: [],
  hintCollapsed: false,
  previewModId: null,
};

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "go":
      return {
        ...state,
        page: action.page,
        visited: { ...state.visited, [action.page]: true },
      };
    case "installed":
      return { ...state, installed: action.list };
    case "select":
      return state.selected.includes(action.id)
        ? { ...state, selected: state.selected.filter((x) => x !== action.id) }
        : { ...state, selected: [...state.selected, action.id] };
    case "clearSelection":
      return { ...state, selected: [] };
    case "setSelection":
      return { ...state, selected: action.ids };
    case "hintCollapsed":
      return { ...state, hintCollapsed: action.value };
    case "preview":
      return { ...state, previewModId: action.id };
  }
}

type Store = State & {
  go: (page: PageKey) => void;
  setInstalled: (list: InstalledMod[]) => void;
  toggleSelect: (id: number) => void;
  setSelection: (ids: number[]) => void;
  clearSelection: () => void;
  setHintCollapsed: (value: boolean) => void;
  /** 打开（传 id）或关掉（传 null）抽屉预览 */
  setPreview: (id: number | null) => void;
};

const StoreContext = createContext<Store | null>(null);

export function AppStoreProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initial);

  const value = useMemo<Store>(
    () => ({
      ...state,
      go: (page) => dispatch({ type: "go", page }),
      setInstalled: (list) => dispatch({ type: "installed", list }),
      toggleSelect: (id) => dispatch({ type: "select", id }),
      setSelection: (ids) => dispatch({ type: "setSelection", ids }),
      clearSelection: () => dispatch({ type: "clearSelection" }),
      setHintCollapsed: (value) => dispatch({ type: "hintCollapsed", value }),
      setPreview: (id) => dispatch({ type: "preview", id }),
    }),
    [state],
  );

  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}

/** 取全局状态。必须在 [`AppStoreProvider`] 里面用。 */
export function useAppStore(): Store {
  const store = useContext(StoreContext);
  if (!store) throw new Error("useAppStore must be used inside AppStoreProvider");
  return store;
}
