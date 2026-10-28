# 岩笋 · Rock Sprout

《深岩银河》（Deep Rock Galactic）的 mod 管理器。搜 mod、一键装进游戏、想卸就卸。

**目前是 v0.1.2，早期版本**：搜索、装、卸、批量操作、导入本地 mod、离线看列表都已跑通并在真机上验过；这一版的主题是「装坏了能救回来」——装之前自动留现场、一键退回、合并包原子写、装之前先看游戏开没开、游戏更新过会提醒，还多了一块「体检」。还没有详情页、没有下载进度百分比，检查新版也没做。

**关于这个项目怎么做的**：代码、文档、界面视觉、测试脚本全部由 AI 编写，人是提需求、拍板方向和验收结果的那个。这不是一个由程序员手工打磨的项目，请按这个前提来看待代码质量与更新节奏。

---

## 现在能做什么

- 粘一个你自己的 mod.io 访问令牌（存本机，不上传）
- 按关键词搜 mod.io 上的 DRG mod
- 点一下就装上：自动下载、合并进游戏的 pak、写入加载器 DLL
- 卸载：从 pak 里移掉并整包重建，游戏自带的文件原样换回
- **批量装 / 批量卸**：勾几个一起处理，底部常驻一条进度条；没成的单独重试，成了的不会被拖回去
- **卸载前先问一句**：把要删的名字和数量逐个列出来，确认了才动手
- **看得出谁盖了谁**：背包里标「被覆盖」/「盖了 N 处」，点开能看到到底是谁盖了谁；还能只看有冲突的
- **装前提醒**：一次装好几个时，会先说清会不会盖掉已经装的东西
- **导入本地 mod**：自己在别处下的 `.pak` / 装着 pak 的 `.zip` / 解包后的文件夹，都能装
- **离线可用**：搜过的列表会存下来，断网时照常看列表、装本地 mod、卸载
- **配置搬运**：导出一份「我装了哪些 mod」的 json（不含令牌），换机器或帮朋友看配置时用
- **诊断信息一键复制**：出问题时点一下，把机器现场贴给别人看
- **装坏了能退回来**：装/卸/导入之前自动存一份现场（合并包 + 已装列表），设置页能自己存保存点，出事点一下退回去
- **合并包不会写半截**：先写临时文件、校验通过才换上去，中途断电也不会把游戏目录搞成「半新半旧」
- **游戏开着就直说**：装、卸、退回之前先看游戏在不在跑，不在「文件被占用」的系统错误里干瞪眼
- **游戏更新过会提醒**：主 pak 变了就提示「得重装一次 mod」，旧备份同时按灰
- **体检**：一键查残留文件、加载器状态、记账和文件对不上、下载目录里的孤儿文件、游戏目录里别的工具留下的 pak
- 看当前登录的是哪个 mod.io 账号
- 用 Steam 启动游戏

## 用之前要知道的

- **游戏必须是 Steam 版**。程序靠 Steam 的库记录找游戏目录，也靠 `steam://` 启动它。Xbox 版没试过。
- **电脑要能访问 mod.io**。搜索和下载全走它的接口，连不上时界面会直说"连不上 mod.io"。
- **需要一个 mod.io 个人访问令牌**：到 <https://mod.io/me/access> 生成，粘进设置页即可。令牌只存在你自己电脑上。
- **会改游戏目录里两个文件**：`FSD\Content\Paks\mods_P.pak`（mod 合并包）和 `FSD\Binaries\Win64\x3daudio1_7.dll`（加载器）。第一次装之前会把游戏自带的那个 DLL 备份到本程序的数据目录，卸载时换回去。
- **出问题怎么办**：到 [Issues](https://github.com/Yisun229/rock-sprout/issues/new/choose) 选「出毛病了」，设置页点「复制诊断信息」粘进去——里面有游戏路径和当前状态，不含令牌。

## 装起来

到 [Releases](https://github.com/Yisun229/rock-sprout/releases) 下最新一版的 `RockSprout_<版本号>_x64-setup.exe`，双击装。同一个页面有 `SHA256SUMS.txt`，想核对文件完整性可以比一下。

安装包没有代码签名，Windows 可能弹 SmartScreen 提示「未知发布者」，点「更多信息 → 仍要运行」即可。

不想装、想自己编：见下面「自己编译」。

---

## 路线图

每一版都锁定一个能验收的目标，不提前做后面的事。

**当前版本 v0.1.2** · 下一版 v0.1.3 轻便

完整路线图在 [`文档\路线图-v0.1到v1.0.md`](文档/路线图-v0.1到v1.0.md)。版本表只在那里维护一份，这里不再重复——同一件事写两处，迟早对不上。

游戏伴侣线（存档看板、职业配装、音乐播放器等）排在 v1.1 之后，见 `文档\路线图-v0.1到v1.0.md`。

**明确不做**：UE4SS 安装、云同步、多游戏支持、启动器、自动更新器、镜像清单、OAuth 授权码流程。

---

## 自己编译

需要：Node 20+ 与 pnpm、Rust nightly、Visual Studio Build Tools（C++ 桌面开发）+ Windows SDK。

```bat
cd app
pnpm install
pnpm run build
node node_modules\@tauri-apps\cli\tauri.js build
```

产物在 `_tools\target\release\`（`CARGO_TARGET_DIR` 指到那里）。打安装包：

```bat
node node_modules\@tauri-apps\cli\tauri.js build
```

**注意**：这个工作区的 Rust 工具链和 Cargo 产物目录都由环境变量重定向到 `_tools\` 下（见下面的"开发工作区"），`cargo` 前要确保那四个环境变量已生效。

改完代码想自测，有两条现成的命令（在仓库根目录跑，需要先编出 `_tools\target\release\rock-sprout.exe`）：

```bat
node _tools\check-all.mjs
node _tools\perf-baseline.mjs
```

第一条会自己开窗口、点界面、调后端，跑完 14 个场景（含断网场景），最后还原游戏目录；加 `--online` 会多跑需要 mod.io 的场景。第二条量三个数写进 `_tools\perf-baseline.txt`：冷启动到界面可用、1000 行渲染、已装 50 个时切页。

---

## 开发工作区

本仓库是项目源码部分。本地开发时还用到这些**不进版本库**的目录：

| 路径 | 用途 |
|---|---|
| `_tools\` | Rust 工具链、VS Build Tools、Cargo 产物、pnpm 存储、游戏目录备份（十几 GB，可重建） |
| `ref\mint\` | [trumank/mint](https://github.com/trumank/mint) 的只读副本，只用来对照行为，不参与构建 |

环境变量（改完要重开终端）：

```bat
setx RUSTUP_HOME "E:\岩笋 · Rock Sprout\_tools\rust"
setx CARGO_HOME  "E:\岩笋 · Rock Sprout\_tools\rust"
setx CARGO_TARGET_DIR "E:\岩笋 · Rock Sprout\_tools\target"
setx PNPM_HOME   "E:\岩笋 · Rock Sprout\_tools\pnpm-store"
```

顶层目录名含中文、空格和「·」，所以**所有参与编译的子目录一律英文小写**（`app`、`ref`、`_tools`），避免 MSVC / Node 对非 ASCII 路径的已知问题。源码在 GitHub 上克隆下来后目录名是英文的，不受这个限制。

## 文档

| 文档 | 内容 |
|---|---|
| `文档\路线图-v0.1到v1.0.md` | v0.1 → v2.0 全部版本、每个版本的硬指标、触发条件 |
| `文档\v0.1-任务清单.md` | v0.1.0 的任务与验收标准，做完了哪些、没做哪些 |
| `文档\v0.1-实施计划.md` | v0.1.0 实际怎么做的、踩了哪些坑、实测数字 |
| `文档\v0.1.1-实施计划.md` | v0.1.1 的计划与交付实况 |
| `文档\v0.1.2-实施计划.md` | v0.1.2 的计划与交付实况 |
| `文档\品牌与UI设计.md` | 品牌、配色、形态原则、页面导航 |

---

## 许可证

- 本项目按 **GPL-3.0** 发布，全文见 `LICENSE`。
- `app\src-tauri\src\integrate.rs`、`app\src-tauri\src\mod_lints\`、`app\src-tauri\mint_lib\`、`app\src-tauri\hook\`、`app\src-tauri\hook_resolvers\`、`app\src-tauri\assets\integration\` 来自 [trumank/mint](https://github.com/trumank/mint)，**MIT** 许可证，全文见 `app\src-tauri\LICENSE-MINT`。
