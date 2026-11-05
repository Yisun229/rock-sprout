# 本目录的对照数据来自哪里

`names.json` 里的资源名与超频名，来自开源项目 **drg-editor**（作者 Alvaro Tinoco）：

- 仓库：https://github.com/MrMarble/drg-editor
- 许可：MIT
- 用到的文件：`src/constant/resources.ts`（资源 GUID → 名字）、`data/schematics.json`（超频 GUID → 名字）

我们把它整理成了自己的格式（只保留 id → 名字），并逐个 GUID 与本机存档核对过；
职业那 4 条不是来自它，路径本来就写在存档里（`/Game/Character/*ID`）。

`weapon-names.json` 不是来自第三方项目，是岩笋自己从本机装的游戏里离线读出来的：

- 来源：游戏自身的 `FSD/Content/Paks/FSD-WindowsNoEditor.pak` 里的物品 ID 资产（`ID_*.uasset` / `.uexp`）
- 做法：按存档里出现的 16 字节 GUID 去 pak 里逐个定位资产，读资产里的 `/Game/Character/<职业>ID`
  与 `/Game/WeaponsNTools/<武器>/WPN_<武器>` 引用，得到「GUID → 武器 → 职业」
- 英文名取资产名；**中文名是人工补的，未经游戏内核对**；查不到的留 null
- 只保留了「GUID → 名字/职业/槽位」这张表，没有包含任何游戏资源文件本身

按 MIT 的要求，下面附上原始许可全文与版权声明。这个声明必须保留：
删掉它就不再合规。

---

MIT License

Copyright (c) 2022 Alvaro Tinoco

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
