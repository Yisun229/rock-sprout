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

/** 一个 mod 的详情，抽屉预览用。字段与后端 src/modio.rs 的 ModDetail 一致。 */
export type ModDetail = {
  id: number;
  name: string;
  summary: string;
  /** 富文本简介（带标签）；面板一律用下面那份纯文本 */
  description: string;
  description_plaintext: string;
  downloads: number;
  version: string;
  /** 封面小图（320×180） */
  logo_320: string;
  /** 封面大图（640×360） */
  logo_640: string;
  /** 作者用户名 */
  creator: string;
  /** 最近一次更新时刻（unix 秒）；0 表示接口没给 */
  date_updated: number;
  /** 只含类型组那 9 个真实标签名 */
  tags: string[];
  /** true = 断网时回放的旧数据 */
  stale: boolean;
  cached_at: number | null;
};

/** 取一个 mod 的详情。断网时给上次存下的那份（`stale` 为 true）。 */
export function getModDetail(id: number): Promise<ModDetail> {
  return call<ModDetail>("get_mod_detail", { id });
}

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
  /** 验身份没成时是哪一类（`network`/`unauthorized`…）；正常是 null */
  code: string | null;
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

/** 「今天」页：一条 Steam 公告 */
export type NewsItem = {
  gid: string;
  title: string;
  url: string;
  /** 发布时刻（unix 秒）；0 表示没解析出来 */
  date: number;
  /** 纯文本正文（已去标签、已按上限截断） */
  contents: string;
};

/** 「今天」页：一条 mod 动态 */
export type ModChange = {
  id: number;
  name: string;
  summary: string;
  downloads: number;
  /** new = 刚上架；updated = 刚更新过 */
  kind: string;
  /** 这件事发生的时刻（unix 秒） */
  when: number;
};

/** 「今天」页：你装的一个 mod */
export type ModUpdate = {
  id: number;
  name: string;
  /** 第一次装上的时刻（unix 毫秒）；老记账没有就是 null */
  installed_at: number | null;
  /** 装上时那个文件的 id（记账里记的） */
  installed_file_id: number;
  /** mod.io 上最新文件的 id；跟上一条不一样就是有新版 */
  latest_file_id: number;
};

/** 下次打开要恢复的界面状态 */
export type UiState = {
  /** 上次停留的页面 id：today / mine / bag / settings */
  page: string | null;
  /** 「粘个令牌」那张卡已收起 */
  token_hint_collapsed: boolean;
};

/** 「今天」页的全部数据 */
export type TodayDigest = {
  /** 这批数据的取得时刻（unix 毫秒） */
  fetched_at: number;
  /** true = 直接用的当天缓存，没发请求 */
  from_cache: boolean;
  news: NewsItem[];
  mod_changes: ModChange[];
  /** 有新版的那几个 */
  updates: ModUpdate[];
  /** 本机一共装着几个 mod（用来区分「都最新」和「一个都没装」） */
  installed_count: number;
  /** 这次真去 mod.io 问成了几个（本地导入的包没法问） */
  checked_count: number;
  /** 本机还没存令牌：mod 那两块空着 */
  token_missing: boolean;
  /** 哪几块没取成，如 ["news"] */
  degraded: string[];
};
