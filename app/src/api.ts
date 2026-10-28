// 和后端说话的统一入口。后端的每个命令都返回 { ok, data, error }，
// 这里把它翻成「成功就返回数据，失败就抛错」，页面里只要 try/catch 一次。
import { invoke } from "@tauri-apps/api/core";

export type ApiResult<T> = {
  ok: boolean;
  data: T | null;
  error: { code: string; message: string } | null;
};

/** 后端给的错误。`code` 用来判断是哪类问题（没令牌、连不上、安装失败…）。 */
export class ApiError extends Error {
  code: string;

  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

export async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const res = await invoke<ApiResult<T>>(command, args);
  if (!res.ok) {
    throw new ApiError(res.error?.code ?? "unknown", res.error?.message ?? "调用失败");
  }
  return res.data as T;
}

/** 把任何异常都收成 [`ApiError`]，界面里只处理一种形状。 */
export function toApiError(e: unknown): ApiError {
  if (e instanceof ApiError) return e;
  return new ApiError("unknown", e instanceof Error ? e.message : String(e));
}

// ---- 后端返回的数据形状（和后端 src/ipc 里的结构体一一对应）----

export type ModSummary = {
  id: number;
  name: string;
  summary: string;
  downloads: number;
};

export type SearchPage = {
  query: string;
  page: number;
  total: number;
  items: ModSummary[];
  /** true = 这是断网时回放的旧数据，不是刚抓的 */
  stale: boolean;
  /** 旧数据是什么时候抓的（unix 毫秒）；stale 为 false 时是 null */
  cached_at: number | null;
};

export type InstalledMod = {
  id: number;
  name: string;
  version: string;
  /** "modio" = 从 mod.io 下的；"local" = 本地导入的 */
  source: string;
  /** 记账指向的文件已经不在了（原文件被删或挪走） */
  missing: boolean;
};

export type InstallOutcome = {
  id: number;
  name: string;
  installed: boolean;
  pak: string | null;
};

/** 预检结果：装这些东西会不会盖掉已经装的。 */
export type PrecheckReport = {
  incoming: string[];
  installed: string[];
  conflicts: { path: string; incoming: string[]; installed: string[] }[];
  /** 一句人话，直接能显示给用户 */
  summary: string;
  warnings: string[];
  elapsed_ms: number;
};

export type TokenStatus = {
  has_token: boolean;
  username: string | null;
  uid: number | null;
  note: string;
};

/** 已经装着的这些 mod 之间，谁盖着谁（一个 mod 一条） */
export type ScanMod = {
  id: number;
  name: string;
  /** 有几个文件被别人盖住了 */
  overridden: number;
  /** 有几个文件盖住了别人 */
  covers: number;
  /** 盖住它的那几个 mod 的名字 */
  by: string[];
};

export type ScanReport = {
  mods: ScanMod[];
  /** 有争议的文件总数 */
  total_paths: number;
  warnings: string[];
  elapsed_ms: number;
};

export type ExportOutcome = {
  /** 配置文件存到哪了 */
  path: string;
  /** 里面一共几个 mod */
  count: number;
};

export type ConfigImportOutcome = {
  path: string;
  total: number;
  only_in_file: string[];
  only_local: string[];
  same: string[];
  /** 一句话结论，界面直接显示 */
  summary: string;
  app_version: string;
  exported_at: number;
};

/** 一份快照（现场备份）：合并包 + 记账在一起 */
export type Snapshot = {
  /** 目录名，回滚和删除时用它 */
  id: string;
  dir: string;
  meta: {
    /** 为什么拍的这份（「装之前」「回滚前」…） */
    reason: string;
    /** 拍的时刻（毫秒时间戳） */
    created_at: number;
    /** 是不是手动保存点（手动的不参与自动清理） */
    manual: boolean;
    /** 当时装着几个 mod */
    mods: number;
    pak_bytes: number;
  };
};

export type RollbackOutcome = {
  /** 退回到了哪一份 */
  id: string;
  /** 退回之后装着几个 mod */
  mods: number;
  /** 退回之前那一刻的现场是哪一份（想再退回来就用它） */
  before: string;
};

/** 游戏本体更新过没有（用主 pak 的大小与修改时间比） */
export type GameCheck = {
  has_game: boolean;
  recorded: boolean;
  /** 主 pak 变了 = 游戏更新过，旧快照别再用 */
  changed: boolean;
  summary: string;
  recorded_size: number | null;
  current_size: number | null;
};

/** 体检报告里的一条 */
export type AuditIssue = {
  /** 分类：residue / dll_missing / dll_stock / dll_backup_lost / record_missing / orphan */
  kind: string;
  title: string;
  detail: string;
  /** 能不能点一下处理掉 */
  fixable: boolean;
  /** 处理动作，fixable 为假时是空串 */
  action: string;
  /** 处理哪个目标（记录 id），不需要目标时是空串 */
  target: string;
};

/** 一次体检的结果 */
export type AuditReport = {
  has_game: boolean;
  game_dir: string;
  mods: number;
  merged_pak: boolean;
  issues: AuditIssue[];
  /** 一句话结论，直接显示给用户 */
  summary: string;
};

/** 数据目录落在哪、为什么 */
export type DataLocation = {
  path: string;
  /** env / portable / fallback */
  mode: string;
  /** 模式的中文说法 */
  label: string;
  portable: boolean;
  /** 程序目录写不了、退到用户目录 —— 界面要常驻提示 */
  fell_back: boolean;
};

/** 旧版本留下的数据（只在扫到时才有） */
export type LegacyCandidate = {
  path: string;
  mods: number;
  snapshots: number;
  has_token: boolean;
};
