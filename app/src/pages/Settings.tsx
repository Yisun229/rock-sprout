// 设置：粘 mod.io 的个人访问令牌。粘完先让后端拿去问一次「这是谁」，验过才存。
import { useEffect, useState } from "react";
import ErrorHint from "../ErrorHint";
import { call, toApiError, type ApiError, type TokenStatus } from "../api";

const ACCESS_PAGE = "https://mod.io/me/access";

export default function Settings() {
  const [status, setStatus] = useState<TokenStatus | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);

  async function refresh() {
    try {
      setStatus(await call<TokenStatus>("token_status"));
    } catch (e) {
      setError(toApiError(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function save() {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      const res = await call<TokenStatus>("save_token", { token: draft });
      setStatus(res);
      setDraft("");
      setNote(res.note);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function clear() {
    setBusy(true);
    setError(null);
    try {
      setStatus(await call<TokenStatus>("clear_token"));
      setNote("已经忘掉令牌了");
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <h1>设置</h1>
      <p className="dim">
        要跟 mod.io 说话就得有一串属于你自己的访问令牌。它只存在这台电脑上，不会发到别处。
      </p>

      <div className="card">
        <div className="card-title">当前状态</div>
        <p className={status?.username ? "note ok" : "note"}>
          {status ? status.note : "读状态…"}
          {status?.uid ? `（uid ${status.uid}）` : ""}
        </p>
      </div>

      <div className="card">
        <div className="card-title">怎么拿令牌</div>
        <ol className="steps">
          <li>
            用浏览器打开 <a href={ACCESS_PAGE}>{ACCESS_PAGE}</a>，登录你的 mod.io 账号
          </li>
          <li>页面上会显示一串「Access Token」，点旁边的复制</li>
          <li>粘到下面这个框里，点「验证并保存」</li>
        </ol>
        <p className="dim small">
          如果打不开这个网址，多半是网络问题——这台机器平时连不上 mod.io，得先开你常用的加速工具。
        </p>
      </div>

      <div className="card">
        <div className="card-title">粘贴令牌</div>
        <textarea
          className="input area"
          rows={3}
          placeholder="把令牌粘在这里"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <div className="searchbar">
          <button
            className="btn"
            disabled={busy || draft.trim() === ""}
            onClick={() => void save()}
          >
            {busy ? "验证中…" : "验证并保存"}
          </button>
          <button className="btn ghost" disabled={busy} onClick={() => void clear()}>
            忘掉令牌
          </button>
        </div>
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} onRetry={() => void refresh()} />}
    </section>
  );
}
