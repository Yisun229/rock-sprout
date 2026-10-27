// 设置：粘 mod.io 的个人访问令牌。粘完先让后端拿去问一次「这是谁」，验过才存。
// 另外管「配置搬运」：导出当前装着哪些 mod，或者拿一份别人的导出来比对。
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import ErrorHint from "../ErrorHint";
import {
  call,
  toApiError,
  type ApiError,
  type ConfigImportOutcome,
  type ExportOutcome,
  type TokenStatus,
} from "../api";

const ACCESS_PAGE = "https://mod.io/me/access";

/**
 * 把文字放进剪贴板，成功返回 true。
 *
 * 窗口没在前台时系统可能一直不给答复（不是报错，是干脆不返回），
 * 所以这里加个上限：超时就当「没写成」，让界面告诉用户自己选中复制，别把按钮卡在「忙」上。
 */
async function writeClipboard(text: string): Promise<boolean> {
  try {
    await Promise.race([
      navigator.clipboard.writeText(text),
      new Promise((_, reject) => setTimeout(() => reject(new Error("超时")), 1200)),
    ]);
    return true;
  } catch {
    return false;
  }
}

export default function Settings() {
  const [status, setStatus] = useState<TokenStatus | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const [diag, setDiag] = useState("");

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

  async function exportConfig() {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      const res = await call<ExportOutcome>("export_config", { dir: null });
      setNote(`配置已导出：${res.path}（${res.count} 个 mod，里面不含令牌）`);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function importConfig() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "岩笋配置", extensions: ["json"] }],
    });
    if (!picked || Array.isArray(picked)) return;

    setBusy(true);
    setError(null);
    setNote("");
    try {
      const res = await call<ConfigImportOutcome>("import_config", { path: picked });
      setNote(`${res.summary}（这份配置是 ${res.app_version} 导出的）`);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function copyDiagnostics() {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      const text = await call<string>("diagnostics");
      setDiag(text);
      if (await writeClipboard(text)) {
        setNote("诊断信息已复制，粘到反馈里就行。里面没有令牌。");
      } else {
        setNote("已经生成在下面，选中复制就行（系统没让直接写剪贴板）。");
      }
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

      <div className="card">
        <div className="card-title">配置搬运</div>
        <p className="dim small">
          导出会生成一个 json，里面只有「装了哪些 mod」的名单，不含令牌。换机器或者帮朋友看配置时用它。
        </p>
        <div className="searchbar">
          <button className="btn" disabled={busy} onClick={() => void exportConfig()}>
            导出配置
          </button>
          <button className="btn ghost" disabled={busy} onClick={() => void importConfig()}>
            读一份配置来比对
          </button>
        </div>
      </div>

      <div className="card">
        <div className="card-title">遇到问题？</div>
        <p className="dim small">
          点一下把「我这台机器什么情况」整理成几行，贴给别人看就行——里面有游戏路径、合并包在不在、加载器是哪个版本，
          没有令牌。
        </p>
        <div className="searchbar">
          <button className="btn" disabled={busy} onClick={() => void copyDiagnostics()}>
            复制诊断信息
          </button>
        </div>
        {diag && <pre className="diag">{diag}</pre>}
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} onRetry={() => void refresh()} />}
    </section>
  );
}
