// 和后端说话的统一入口。后端的每个命令都返回 { ok, data, error }，
// 这里把它翻成「成功就返回数据，失败就抛错」，页面里只要 try/catch 一次。
import { invoke } from "@tauri-apps/api/core";

export type ApiResult<T> = {
  ok: boolean;
  data: T | null;
  error: { code: string; message: string } | null;
};

/** 「我的数据」页要的几个数：概览只摆这四个。 */
export type SaveNumbers = {
  credits: number;
  play_seconds: number;
  games_played: number;
  slots_total: number;
  slots_active: number;
  slots_idle: number;
};

/** 存档选择器里的一项。 */
export type SaveCandidate = {
  id: string;
  account: string;
  games: number;
  modified_ms: number;
  active: boolean;
};

/** 存档选择器要的清单。 */
export type SaveList = {
  items: SaveCandidate[];
  selected: string | null;
};

/** 存档概览。`state` 只有三种值：ok / not_found / unknown_version / bad_structure。 */
export type SaveOverview = {
  state: string;
  note: string | null;
  file_name: string | null;
  from_cache: boolean;
  numbers: SaveNumbers | null;
  resources_total: number;
  resources: { id: string; count: number }[];
  blocks_read: number;
  blocks_pending: number;
};

/** 开发者视图的一行 */
export type FieldRow = {
  path: string;
  ty: string;
  value: string;
  known: boolean;
};

/** 开发者视图的全部内容（默认关着，设置页开关打开才会请求） */
export type SaveDebug = {
  state: string;
  note: string;
  file_name: string | null;
  sha256: string;
  file_bytes: number;
  nodes: number;
  consumed: number;
  skipped: number;
  notes: string[];
  rows: FieldRow[];
  rows_hidden: number;
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
  /** "pak" = 进合并包的数据包；"script" = 靠 UE4SS 跑的脚本类，单独一个目录 */
  kind: string;
  /** 脚本类专用：`ok` / `failed` / `unknown`（数据包是空串） */
  script_state: string;
  /** 脚本类专用：没生效时的一句话原因 */
  script_detail: string;
};

/** 读一次 UE4SS 现在的状态（和「今天」页看到的是同一个判断）。 */
export function ue4ssHealth(): Promise<Ue4ssHealth> {
  return call<Ue4ssHealth>("ue4ss_health");
}

export type InstallOutcome = {
  id: number;
  name: string;
  installed: boolean;
  pak: string | null;
  /** 导入成功但没装进游戏时的原因，目前只有 o_game */
  reason?: string | null;
};

/** 换成 mod.io 上的最新版（只对从 mod.io 装的 mod 有用）。 */
export function upgradeMod(id: number): Promise<InstallOutcome> {
  return call<InstallOutcome>("upgrade_mod", { id });
}

/** 一次把能换的都换掉；有任何一条失败就整体放弃，旧版全留着。 */
export function upgradeAll(): Promise<InstallOutcome[]> {
  return call<InstallOutcome[]>("upgrade_all");
}

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

/** 「我的矿工」里一个槽位。`level` 为 null 表示曲线对不上，界面退回显示 XP */
export type DwarfCharacter = {
  slot: number;
  xp: number;
  level: number | null;
  times_retired: number;
  retired_levels: number;
  has_progress: boolean;
  /** 主副武器反推出来的职业；认不出是 null */
  class: string | null;
  /** 凭什么这么判（写给人看的一行） */
  basis: string | null;
};

/** 「我的矿工」里一条里程碑。`name` 为 null 表示名字没对上，界面显示 #id 前 8 位 */
export type DwarfMilestone = {
  id: string;
  name: string | null;
  /** 把 {value} / {character} 填好之后的显示名 */
  display: string;
  tier: number;
  max_tier: number;
  next_perk_points: number | null;
};

/** 角色块 + 里程碑块（后端聚合，前端不写死任何名字或数字） */
export type DwarfView = {
  state: string;
  note: string;
  file_name: string | null;
  from_cache: boolean;
  curve_ok: boolean;
  characters: DwarfCharacter[];
  milestones: DwarfMilestone[];
  milestones_claimed: number;
  milestones_total: number;
};

/** 一条改装：买没买、有没有正装着 */
export type UpgradeItem = {
  id: string;
  name: string;
  name_en: string | null;
  owned: boolean;
  equipped: boolean;
  /** false = 正装着但改装表里没有，界面要明写一句 */
  in_table: boolean;
};

/** 正装着的那条超频。稀有度只显示文字，不加颜色 */
export type OverclockItem = {
  id: string;
  name: string;
  name_en: string | null;
  rarity: string | null;
};

/** 「装备」页里的一格。`id` 为空串表示这个槽没装东西 */
export type EquipmentItem = {
  key: string;
  id: string;
  name: string;
  name_en: string | null;
  known: boolean;
  class: string | null;
  slot_kind: string | null;
  /** 正装着的那条超频，没装是 null */
  overclock: OverclockItem | null;
  /** 存档里这只武器开没开超频 */
  oc_open: boolean;
  /** 这把武器一共有几条超频（只给数字） */
  oc_total: number;
  /** 这件东西的改装；没有改装表时是空数组 */
  upgrades: UpgradeItem[];
};

/** 一套配装：八格固定顺序 */
export type EquipmentLoadout = {
  index: number;
  items: EquipmentItem[];
};

/** 一个角色槽：职业（可能认不出）+ 三套配装 */
export type CharacterEquip = {
  slot: number;
  class: string | null;
  basis: string | null;
  loadouts: EquipmentLoadout[];
};

/** 装备页数据 */
export type OwnedNameRow = { id: string; name: string; source: string; owned: boolean };

export type EquipmentView = {
  state: string;
  note: string;
  file_name: string | null;
  from_cache: boolean;
  characters: CharacterEquip[];
  /** 改装那块的一句脚注：解锁与否是整份存档一份清单，不分武器 */
  upgrade_note: string;
  skins: OwnedNameRow[];
  pickaxe: OwnedNameRow[];
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
  /** UE4SS 有没有在起作用（每次打开现算，不进缓存） */
  ue4ss: Ue4ssHealth;
};
/** UE4SS 生效检测（M7-3） */
export type Ue4ssHealth = {
  /** not_installed / unknown / ok / broken */
  state: string;
  /** 给人看的一句话 */
  detail: string;
};
/** UE4SS 这套运行时的准备情况 */
export type Ue4ssStatus = {
  /** 钉死的版本号，比如 v3.0.1 */
  version: string;
  /** 官方下载地址（手动下载时给用户） */
  url: string;
  /** 压缩包应有的字节数 */
  zip_size: number;
  /** 压缩包现在在哪儿 */
  zip_path: string;
  /** 压缩包已经在本地、而且核对过大小和指纹 */
  zip_ready: boolean;
  /** 手动下载时把 zip 放进这个目录 */
  manual_dir: string;
  /** 解出来的四样东西齐了没有 */
  package_ready: boolean;
  /** 解出来的包放在哪儿 */
  package_dir: string;
  /** 正在下载 */
  downloading: boolean;
  /** 已下多少字节 */
  downloaded: number;
};

/** 看 UE4SS 现在到哪一步了（纯读，可以每 300 ms 问一次） */
export function ue4ssStatus(): Promise<Ue4ssStatus> {
  return call<Ue4ssStatus>("ue4ss_status", {});
}

/** 备齐 UE4SS：本地有就解包，没有就下载；已备齐直接返回 */
export function ue4ssPrepare(): Promise<Ue4ssStatus> {
  return call<Ue4ssStatus>("ue4ss_prepare", {});
}

/** 取消正在进行的下载 */
export function ue4ssCancel(): Promise<boolean> {
  return call<boolean>("ue4ss_cancel", {});
}

/** 游戏目录里现在这套 UE4SS 是什么来路 */
export type Ue4ssExisting = {
  /** none = 干干净净；ours = 我们铺的；other = 别人那套 */
  kind: "none" | "ours" | "other";
  /** 给人看的说明 */
  detail: string;
};

/** 装完 / 卸完之后的回报 */
export type Ue4ssInstallOutcome = {
  items: string[];
  /** 备份目录（没备份过就是空串） */
  backup_dir: string;
  existing: Ue4ssExisting;
};

/** 游戏目录里现在有什么（只读） */
export function ue4ssGameStatus(): Promise<Ue4ssExisting> {
  return call<Ue4ssExisting>("ue4ss_game_status", {});
}

/** 把官方那四样铺进游戏目录；已有别人那套时必须显式确认 */
export function ue4ssInstall(confirmReplace: boolean): Promise<Ue4ssInstallOutcome> {
  return call<Ue4ssInstallOutcome>("ue4ss_install", { confirmReplace });
}

/** 撤掉我们铺的，把备份原样放回去 */
export function ue4ssUninstall(): Promise<Ue4ssInstallOutcome> {
  return call<Ue4ssInstallOutcome>("ue4ss_uninstall", {});
}