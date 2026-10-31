// 背包要的数据与动作：读已装列表、卸载、导入本地文件/文件夹。
//
// 抽成 hook 是为了让背包页只剩「排版」；也方便以后别的页要用同一份列表。
import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  call,
  toApiError,
  type ApiError,
  type InstallOutcome,
  type InstalledMod,
  type ScanReport,
} from "../api";
import { t } from "../i18n";
import { useAppStore } from "../store/appStore";

export type InstalledState = {
  items: InstalledMod[] | null;
  /** 这些 mod 之间的覆盖关系（谁盖了谁）。没扫过就是 null */
  conflicts: ScanReport | null;
  busy: boolean;
  note: string;
  error: ApiError | null;
  /** 脚本类撞上同名目录时记在这里，界面拿去问「换个名字还是替换」 */
  conflict: { path: string; name: string; message: string } | null;
  clearConflict: () => void;
  refresh: () => Promise<void>;
  remove: (mod: InstalledMod) => Promise<void>;
  /** 只删掉一条空记账（原文件早就不在了） */
  dropRecord: (mod: InstalledMod) => Promise<void>;
  importPaths: (paths: string[]) => Promise<void>;
  /** 撞名之后换个名字再试一次 */
  importAs: (path: string, dirName: string) => Promise<void>;
  /** 撞名之后替换掉旧的（旧的先备份） */
  importReplace: (path: string) => Promise<void>;
  pickFiles: () => Promise<void>;
  pickFolder: () => Promise<void>;
};

export function useInstalled(): InstalledState {
  const { setInstalled } = useAppStore();
  const [items, setItems] = useState<InstalledMod[] | null>(null);
  const [conflicts, setConflicts] = useState<ScanReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const [conflict, setConflict] = useState<{ path: string; name: string; message: string } | null>(
    null,
  );
  const started = useRef(false);

  const refresh = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const list = await call<InstalledMod[]>("list_installed");
      setItems(list);
      setInstalled(list);
      // 顺手把覆盖关系也算一遍：这活儿在 Rust 那边，有清单缓存，通常几十毫秒
      if (list.length > 1) {
        setConflicts(await call<ScanReport>("scan_conflicts"));
      } else {
        setConflicts(null);
      }
    } catch (e) {
      setError(toApiError(e));
      setItems([]);
    } finally {
      setBusy(false);
    }
  }, [setInstalled]);

  // 开发模式下 StrictMode 会把挂载流程跑两遍，这里挡一下，别让同一个请求发两次。
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void refresh();
  }, [refresh]);

  const remove = useCallback(
    async (mod: InstalledMod) => {
      setBusy(true);
      setError(null);
      setNote(t("bag.removing", { name: mod.name }));
      try {
        const res = await call<InstallOutcome>("uninstall_mod", { id: mod.id });
        setNote(res.pak ? t("bag.removedMore", { name: res.name }) : t("bag.removedLast", { name: res.name }));
        await refresh();
      } catch (e) {
        setNote("");
        setError(toApiError(e));
        setBusy(false);
      }
    },
    [refresh],
  );

  // 原文件早就没了的那种：只把这条空记账删掉，不碰游戏目录，也不用重建合并包
  const dropRecord = useCallback(
    async (mod: InstalledMod) => {
      setBusy(true);
      setError(null);
      try {
        const name = await call<string>("delete_record_only", { id: mod.id });
        setNote(t("bag.droppedRecord", { name }));
        await refresh();
      } catch (e) {
        setNote("");
        setError(toApiError(e));
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  const importPaths = useCallback(
    async (paths: string[], opts?: { replace?: boolean; dirName?: string }) => {
      setBusy(true);
      setError(null);
      setConflict(null);
      let last = "";
      try {
        for (const path of paths) {
          setNote(t("bag.importing", { path }));
          const res = await call<InstallOutcome>("import_local", {
            path,
            replace: opts?.replace ?? false,
            dirName: opts?.dirName ?? null,
          });
          last = res.name;
        }
        setNote(
          paths.length === 1
            ? t("bag.importedOne", { name: last })
            : t("bag.importedMany", { count: paths.length, name: last }),
        );
        await refresh();
      } catch (e) {
        const err = toApiError(e);
        // 脚本类撞上同名目录：不算错误，问用户「换个名字」还是「替换」
        if (err.code === "exists" && paths.length === 1) {
          setNote("");
          setConflict({ path: paths[0], name: baseName(paths[0]), message: err.message });
        } else {
          setNote("");
          setError(err);
        }
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  const importAs = useCallback(
    async (path: string, dirName: string) => {
      setConflict(null);
      await importPaths([path], { dirName });
    },
    [importPaths],
  );

  const importReplace = useCallback(
    async (path: string) => {
      setConflict(null);
      await importPaths([path], { replace: true });
    },
    [importPaths],
  );

  const pickFiles = useCallback(async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: t("bag.pickFiles"), extensions: ["pak", "zip"] }],
    });
    if (!picked) return;
    await importPaths(Array.isArray(picked) ? picked : [picked]);
  }, [importPaths]);

  const pickFolder = useCallback(async () => {
    const picked = await open({ directory: true, multiple: false });
    if (!picked || Array.isArray(picked)) return;
    await importPaths([picked]);
  }, [importPaths]);

  return {
    items,
    conflicts,
    busy,
    note,
    error,
    conflict,
    clearConflict: () => setConflict(null),
    refresh,
    remove,
    dropRecord,
    importPaths,
    importAs,
    importReplace,
    pickFiles,
    pickFolder,
  };
}

/** 从路径里抠出一个像样的名字：既是目录名也是界面上默认的「换个名字」。 */
function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  const last = parts[parts.length - 1] ?? "script-mod";
  return last.replace(/\.zip$/i, "");
}
