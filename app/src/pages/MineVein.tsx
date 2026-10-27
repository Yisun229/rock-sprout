// 矿脉：找 mod、装 mod。装的时候后端会下载好、合并进游戏，装完直接能进游戏看到。
import { useState } from "react";
import ErrorHint from "../ErrorHint";
import {
  call,
  toApiError,
  type ApiError,
  type InstallOutcome,
  type SearchPage,
} from "../api";

function thousands(n: number): string {
  return n.toLocaleString("zh-CN");
}

/** 缓存时间（unix 毫秒）→「10 月 27 日 18:43」。取不到时间就说「上次」。 */
function whenText(ms: number | null): string {
  if (!ms) return "上次";
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getMonth() + 1} 月 ${d.getDate()} 日 ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

type Props = {
  onGoSettings: () => void;
};

export default function MineVein({ onGoSettings }: Props) {
  const [query, setQuery] = useState("");
  const [page, setPage] = useState<SearchPage | null>(null);
  const [busy, setBusy] = useState(false);
  const [installing, setInstalling] = useState<number | null>(null);
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const [retry, setRetry] = useState<() => void>(() => () => {});

  async function doSearch(nextPage: number) {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      const res = await call<SearchPage>("search_mods", { query, page: nextPage });
      setPage(res);
    } catch (e) {
      setPage(null);
      setError(toApiError(e));
      setRetry(() => () => void doSearch(nextPage));
    } finally {
      setBusy(false);
    }
  }

  async function doInstall(id: number, name: string) {
    setInstalling(id);
    setError(null);
    setNote(`正在装「${name}」：下载 → 合并进游戏，第一次装会慢一点`);
    try {
      const res = await call<InstallOutcome>("install_mod", { id });
      setNote(
        res.pak
          ? `装好了：${res.name}。已经写进 ${res.pak}，进游戏就能看到`
          : `装好了：${res.name}`,
      );
    } catch (e) {
      setNote("");
      setError(toApiError(e));
      setRetry(() => () => void doInstall(id, name));
    } finally {
      setInstalling(null);
    }
  }

  return (
    <section>
      <h1>矿脉</h1>
      <p className="dim">搜 mod.io 上的《深岩银河》mod，点「装」直接进游戏。</p>

      <div className="searchbar">
        <input
          className="input"
          placeholder="输入关键词，比如 drill、亮、地图"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void doSearch(1);
          }}
        />
        <button className="btn" disabled={busy} onClick={() => void doSearch(1)}>
          {busy ? "搜…" : "搜索"}
        </button>
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && (
        <ErrorHint error={error} onRetry={retry} onGoSettings={onGoSettings} />
      )}

      {page?.stale && (
        <div className="banner warn">
          <span>
            当前离线，以下是 {whenText(page.cached_at)} 抓到的数据，可能已经过期。
          </span>
          <button
            className="btn small ghost"
            disabled={busy}
            onClick={() => void doSearch(page.page)}
          >
            {busy ? "重试中…" : "重试"}
          </button>
        </div>
      )}

      {page && page.items.length > 0 && (
        <>
          <p className="dim small">
            {query.trim() ? `「${page.query}」` : "热门"} 共 {thousands(page.total)} 个，第{" "}
            {page.page} 页
          </p>
          <ul className="list">
            {page.items.map((m) => (
              <li key={m.id} className="row">
                <div className="row-main">
                  <div className="row-title">{m.name}</div>
                  {m.summary && <div className="row-sub">{m.summary}</div>}
                </div>
                <div className="row-side">
                  <span className="badge">↓ {thousands(m.downloads)}</span>
                  <button
                    className="btn small"
                    disabled={installing !== null}
                    onClick={() => void doInstall(m.id, m.name)}
                  >
                    {installing === m.id ? "装着…" : "装"}
                  </button>
                </div>
              </li>
            ))}
          </ul>

          <div className="pager">
            <button
              className="btn ghost"
              disabled={busy || page.page <= 1}
              onClick={() => void doSearch(page.page - 1)}
            >
              上一页
            </button>
            <button
              className="btn ghost"
              disabled={busy || page.items.length === 0}
              onClick={() => void doSearch(page.page + 1)}
            >
              下一页
            </button>
          </div>
        </>
      )}

      {page && page.items.length === 0 && (
        <p className="dim">一个都没搜到，换个词试试。</p>
      )}
      {!page && !busy && !error && (
        <p className="dim">还没搜过。直接点「搜索」可以看热门。</p>
      )}
    </section>
  );
}
