// 设置页的第五张卡：体检。
//
// 把「该在的不在、不该在的还在」一次列清楚：残留的临时文件、加载器状态、
// 记录还在但文件没了、磁盘上有文件却没人认领。
// 能处理的给按钮，处理不了的说清后果——但不删任何用户数据（孤儿文件是挪进备份目录）。
import { useEffect, useRef, useState } from "react";
import ErrorHint from "../../ErrorHint";
import { call, toApiError, type ApiError, type AuditIssue, type AuditReport } from "../../api";
import { t } from "../../i18n";

export default function AuditCard() {
  const [report, setReport] = useState<AuditReport | null>(null);
  const [busy, setBusy] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const started = useRef(false);

  async function refresh() {
    try {
      const r = await call<AuditReport>("audit_game");
      setReport(r);
      setError(null);
    } catch (e) {
      setError(toApiError(e));
    }
  }

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void refresh();
  }, []);

  async function fix(issue: AuditIssue) {
    setBusy(issue.action + issue.target);
    setNote("");
    setError(null);
    try {
      const said = await call<string>("fix_audit_issue", {
        action: issue.action,
        target: issue.target,
      });
      setNote(said);
      await refresh();
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy("");
    }
  }

  return (
    <div className="card">
      <div className="card-title">{t("audit.title")}</div>
      <p className="note">{t("audit.hint")}</p>

      {error && <ErrorHint error={error} onRetry={() => void refresh()} />}

      {report && (
        <>
          <p className="note small dim">{report.summary}</p>
          {report.issues.length === 0 && <p className="note ok">{t("audit.clean")}</p>}
          <ul className="audit">
            {report.issues.map((issue) => (
              <li key={issue.kind + issue.target} className={issue.fixable ? "audit-item" : "audit-item info"}>
                <div className="audit-main">
                  <div className="audit-title">{issue.title}</div>
                  <div className="audit-detail">{issue.detail}</div>
                </div>
                {issue.fixable && (
                  <button
                    className="btn small"
                    disabled={busy !== ""}
                    onClick={() => void fix(issue)}
                  >
                    {busy === issue.action + issue.target ? t("audit.fixing") : t("audit.fix")}
                  </button>
                )}
              </li>
            ))}
          </ul>
        </>
      )}

      <div className="searchbar">
        <button className="btn small ghost" disabled={busy !== ""} onClick={() => void refresh()}>
          {t("audit.again")}
        </button>
      </div>
      {note && <p className="note ok">{note}</p>}
    </div>
  );
}
