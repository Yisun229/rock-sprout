// 覆盖关系抽屉：把「谁盖了谁」一次摊开说清。
//
// 只在用户主动点开时出现，不打扰正常使用。只列真有重叠的 mod，
// 公共文件（AssetRegistry.bin、_Interop）在检测那一步已经排除了。
import { t } from "../i18n";
import type { ScanReport } from "../api";

type Props = {
  report: ScanReport | null;
  onClose: () => void;
};

export default function ConflictDrawer({ report, onClose }: Props) {
  const rows = (report?.mods ?? []).filter((m) => m.overridden > 0 || m.covers > 0);
  return (
    <div className="drawer">
      <div className="drawer-card">
        <div className="modal-title">{t("conflict.drawerTitle")}</div>
        <p className="dim small">{t("conflict.drawerLead")}</p>

        {rows.length === 0 && <p className="dim">{t("conflict.drawerEmpty")}</p>}

        {rows.length > 0 && (
          <ul className="drawer-list">
            {rows.map((m) => (
              <li key={m.id} className="drawer-item">
                <div className="row-title">{m.name}</div>
                {m.overridden > 0 && (
                  <div className="drawer-note bad">
                    {t("conflict.drawerItem", { names: m.by.join("、"), count: m.overridden })}
                  </div>
                )}
                {m.covers > 0 && (
                  <div className="drawer-note">{t("conflict.drawerCovers", { count: m.covers })}</div>
                )}
              </li>
            ))}
          </ul>
        )}

        <div className="modal-actions">
          <button className="btn small" onClick={onClose}>
            {t("conflict.close")}
          </button>
        </div>
      </div>
    </div>
  );
}
