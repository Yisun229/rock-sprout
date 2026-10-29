// 关于：图标、版本号、项目主页、许可。
//
// 不单独占一页——左侧导航的三页各管一堆事，关于只是看一眼就走，
// 做成弹层（跟确认框同一套样式），免得为它再动导航和全局状态。
import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { call, toApiError, type ApiError } from "../api";
import ErrorHint from "../ErrorHint";
import { t } from "../i18n";

const REPO = "https://github.com/Yisun229/rock-sprout";

export default function AboutDialog({ onClose }: { onClose: () => void }) {
  const [version, setVersion] = useState("…");
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  useEffect(() => {
    void getVersion()
      .then(setVersion)
      .catch(() => setVersion(t("about.versionUnknown")));
  }, []);

  async function openRepo() {
    setOpening(true);
    setError(null);
    try {
      await call<string>("open_url", { url: REPO });
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setOpening(false);
    }
  }

  return (
    <div className="modal">
      <div className="modal-card">
        <div className="about-head">
          <img className="about-icon" src="/icon-128.png" width={64} height={64} alt="" />
          <div>
            <div className="modal-title">{t("about.title")}</div>
            <p className="dim small">{t("about.version", { version })}</p>
          </div>
        </div>
        <p className="dim small">{t("about.blurb")}</p>
        <p className="note">{t("about.license")}</p>
        {error && <ErrorHint error={error} />}
        <div className="modal-actions">
          <button className="btn ghost small" disabled={opening} onClick={() => void openRepo()}>
            {t("about.repo")}
          </button>
          <button className="btn small" onClick={onClose}>
            {t("about.close")}
          </button>
        </div>
      </div>
    </div>
  );
}
