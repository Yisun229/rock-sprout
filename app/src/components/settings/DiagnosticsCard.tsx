// 设置页的第三张卡：一键把「这台机器什么情况」整理成几行，方便贴给别人看。
import { useState } from "react";
import ErrorHint from "../../ErrorHint";
import { call, toApiError, type ApiError } from "../../api";
import { writeClipboard } from "../../clipboard";
import { t } from "../../i18n";

/// 报告问题时正文最多写这么多字：GitHub 那条链接太长会被掐断。
const BODY_LIMIT = 1000;
const NEW_ISSUE = "https://github.com/Yisun229/rock-sprout/issues/new";

/// 从诊断信息里挑出有用的几行，拼成一条能直接贴的正文。
export function buildIssueBody(diagnostics: string): string {
  const lines = diagnostics.split("\n");
  const pick = (prefix: string) => lines.find((line) => line.startsWith(prefix)) ?? `（没读到${prefix}）`;
  const head = [
    "**我遇到了什么**",
    "（在这里写一句：卡在哪一步、看到什么提示）",
    "",
    "**环境**",
    `- ${pick("岩笋 ")}`,
    `- ${pick("系统：")}`,
    `- ${pick("数据目录：")}`,
    `- ${pick("游戏目录：")}`,
  ].join("\n");
  const tail = "\n\n完整诊断信息我点了「复制诊断信息」，需要的话再补上来（里面没有令牌）。";
  const room = BODY_LIMIT - tail.length;
  return (head.length > room ? head.slice(0, room) : head) + tail;
}

export function issueUrl(diagnostics: string): string {
  const title = "遇到了一个问题（岩笋 " + versionOf(diagnostics) + "）";
  const body = buildIssueBody(diagnostics);
  return `${NEW_ISSUE}?title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`;
}

function versionOf(diagnostics: string): string {
  const first = diagnostics.split("\n")[0] ?? "";
  return first.replace("岩笋", "").trim() || "未知版本";
}

export default function DiagnosticsCard() {
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [diag, setDiag] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const [reporting, setReporting] = useState(false);

  async function copyDiagnostics() {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      const text = await call<string>("diagnostics");
      setDiag(text);
      setNote(
        (await writeClipboard(text)) ? t("settings.diagCopied") : t("settings.diagNotCopied"),
      );
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function reportProblem() {
    setReporting(true);
    setError(null);
    setNote("");
    try {
      const text = await call<string>("diagnostics");
      setDiag(text);
      await call<string>("open_url", { url: issueUrl(text) });
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setReporting(false);
    }
  }

  return (
    <>
      <div className="card">
        <div className="card-title">{t("settings.diag")}</div>
        <p className="dim small">{t("settings.diagHint")}</p>
        <div className="searchbar">
          <button className="btn" disabled={busy} onClick={() => void copyDiagnostics()}>
            {t("action.copyDiagnostics")}
          </button>
          <button
            className="btn ghost"
            disabled={reporting}
            onClick={() => void reportProblem()}
            title={t("settings.reportHint")}
          >
            {reporting ? t("settings.reporting") : t("settings.report")}
          </button>
        </div>
        {diag && <pre className="diag">{diag}</pre>}
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} />}
    </>
  );
}
