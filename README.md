# 岩笋 · Rock Sprout

《深岩银河》（Deep Rock Galactic）的 mod 管理器。搜 mod、一键装进游戏、想卸就卸。

**目前是 v0.1，早期版本**：核心链路（搜索 → 下载 → 合并进游戏 → 卸载还原）已经跑通并在真机上验过，但功能还很薄——没有详情页、没有下载进度条。把它当"能用的第一版"，不是完成品。

**关于这个项目怎么做的**：代码、文档、界面视觉、测试脚本全部由 AI 编写，人是提需求、拍板方向和验收结果的那个。这不是一个由程序员手工打磨的项目，请按这个前提来看待代码质量与更新节奏。

---

## 现在能做什么

- 粘一个你自己的 mod.io 访问令牌（存本机，不上传）
- 按关键词搜 mod.io 上的 DRG mod
- 点一下就装上：自动下载、合并进游戏的 pak、写入加载器 DLL
- 卸载：从 pak 里移掉并整包重建，游戏自带的文件原样换回
- **导入本地 mod**：自己在别处下的 `.pak` / 装着 pak 的 `.zip` / 解包后的文件夹，都能装
- **离线可用**：搜过的列表会存下来，断网时照常看列表、装本地 mod、卸载
- **配置搬运**：导出一份「我装了哪些 mod」的 json（不含令牌），换机器或帮朋友看配置时用
- **诊断信息一键复制**：出问题时点一下，把机器现场贴给别人看
- 看当前登录的是哪个 mod.io 账号
- 用 Steam 启动游戏

## 用之前要知道的

- **游戏必须是 Steam 版**。程序靠 Steam 的库记录找游戏目录，也靠 `steam://` 启动它。Xbox 版没试过。
- **电脑要能访问 mod.io**。搜索和下载全走它的接口，连不上时界面会直说"连不上 mod.io"。
- **需要一个 mod.io 个人访问令牌**：到 <https://mod.io/me/access> 生成，粘进设置页即可。令牌只存在你自己电脑上。
- **会改游戏目录里两个文件**：`FSD\Content\Paks\mods_P.pak`（mod 合并包）和 `FSD\Binaries\Win64\x3daudio1_7.dll`（加载器）。第一次装之前会把游戏自带的那个 DLL 备份到本程序的数据目录，卸载时换回去。
- **出问题怎么办**：到 [Issues](https://github.com/Yisun229/rock-sprout/issues/new/choose) 选「出毛病了」，设置页点「复制诊断信息」粘进去——里面有游戏路径和当前状态，不含令牌。

## 装起来

到 [Releases](https://github.com/Yisun229/rock-sprout/releases) 下 `RockSprout_0.1.0_x64-setup.exe`，双击装。

安装包没有代码签名，Windows 可能弹 SmartScreen 提示「未知发布者」，点「更多信息 → 仍要运行」即可。

不想装、想自己编：见下面「自己编译」。

---

## 路线图

每一版都锁定一个能验收的目标，不提前做后面的事。

| 版本 | 一句话目标 | 比同类工具好在哪 |
|---|---|---|
| **v0.1 能装**（当前） | 搜到、装上、进游戏生效 | 令牌验证后取你自己的 uid，不跟所有人挤同一条限速队列 |
| **v0.2 好管** | 不止装一个，能管一堆 | 批量装卸；谁覆盖谁一眼看清；本地缓存与配置导入导出 |
| **v0.3 敢用** | 装坏了能救回来 | 装之前自动留快照，一键回到上一个状态 |
| **v0.4 轻便** | 轻、快、能塞 U 盘 | 绿色版，配置跟着文件夹走 |
| **v0.5 好看** | 视觉打磨到位 | 尖角 + 哑光 + 矿琥珀主色，一眼看出不是别家 |
| **v0.6 / v0.7** | 按实际需求触发，不排死期 | — |
| **v1.0 定型** | 稳定 + 文档 + 开源治理 | GPL-3.0：谁都能改、能发、能自己做 |

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
| `文档\v0.1-任务清单.md` | v0.1 的任务与验收标准，做完了哪些、没做哪些 |
| `文档\v0.1-实施计划.md` | 实际怎么做的、踩了哪些坑、实测数字 |
| `文档\品牌与UI设计.md` | 品牌、配色、形态原则、页面导航 |

---

## 许可证

- 本项目按 **GPL-3.0** 发布，全文见 `LICENSE`。
- `app\src-tauri\src\integrate.rs`、`app\src-tauri\src\mod_lints\`、`app\src-tauri\mint_lib\`、`app\src-tauri\hook\`、`app\src-tauri\hook_resolvers\`、`app\src-tauri\assets\integration\` 来自 [trumank/mint](https://github.com/trumank/mint)，**MIT** 许可证，全文见 `app\src-tauri\LICENSE-MINT`。
