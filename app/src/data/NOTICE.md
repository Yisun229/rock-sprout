# 本目录的对照数据来自哪里

`names.json` 里的资源名与超频名，来自开源项目 **drg-editor**（作者 Alvaro Tinoco）：

- 仓库：https://github.com/MrMarble/drg-editor
- 许可：MIT
- 用到的文件：`src/constant/resources.ts`（资源 GUID → 名字）、`data/schematics.json`（超频 GUID → 名字）

我们把它整理成了自己的格式（只保留 id → 名字），并逐个 GUID 与本机存档核对过；
职业那 4 条不是来自它，路径本来就写在存档里（`/Game/Character/*ID`）。

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
