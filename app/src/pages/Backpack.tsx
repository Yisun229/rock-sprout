// 背包：已经装进游戏的 mod。卸载后后端会立刻重建合并包。
import { useEffect, useState } from "react";
import ErrorHint from "../ErrorHint";
import {
  call,
  toApiError,
  type ApiError,
  type InstallOutcome,
  type InstalledMod,
} from "../api";

export default function Backpack() {
  const [items, setItems] = useState<InstalledMod[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);

  async function refresh() {
    setBusy(true);
    setError(null);
    try {
      setItems(await call<InstalledMod[]>("list_installed"));
    } catch (e) {
      setError(toApiError(e));
      setItems([]);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function remove(id: number, name: string) {
    setBusy(true);
    setError(null);
    setNote(`正在卸「${name}」，需要重新生成合并包`);
    try {
      const res = await call<InstallOutcome>("uninstall_mod", { id });
      setNote(
        res.pak
          ? `卸掉了：${res.name}。现在游戏里还有别的 mod`
          : `卸掉了：${res.name}。这是最后一个，已经清干净，游戏回到没装 mod 的样子`,
      );
      await refresh();
    } catch (e) {
      setNote("");
      setError(toApiError(e));
      setBusy(false);
    }
  }

  return (
    <section>
      <h1>背包</h1>
      <p className="dim">装着的东西都在这儿。卸掉会立刻重建游戏里的合并包。</p>

      <div className="searchbar">
        <button className="btn ghost" disabled={busy} onClick={() => void refresh()}>
          {busy ? "读…" : "刷新"}
        </button>
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} onRetry={() => void refresh()} />}

      {items && items.length === 0 && (
        <p className="dim">现在一个都没装。去「矿脉」挑一个吧。</p>
      )}

      {items && items.length > 0 && (
        <ul className="list">
          {items.map((m) => (
            <li key={m.id} className="row">
              <div className="row-main">
                <div className="row-title">{m.name}</div>
                <div className="row-sub">文件：{m.version}</div>
              </div>
              <div className="row-side">
                <button
                  className="btn ghost small"
                  disabled={busy}
                  onClick={() => void remove(m.id, m.name)}
                >
                  卸载
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
