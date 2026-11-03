// 开发者视图的开关。默认关，只在这一运行里有效——不写进 ui-state，
// 免得下次打开莫名其妙看到内部字段名。它也不进导航，只有翻到设置页最底下才会看见。
import { useAppStore } from "../../store/appStore";
import { t } from "../../i18n";

export default function DevCard() {
  const { devView, setDevView } = useAppStore();
  return (
    <div className="card dev-card">
      <div className="card-title">{t("dev.title")}</div>
      <p className="dim small" style={{ marginTop: 0 }}>
        {t("dev.lead")}
      </p>
      <label className="checkline">
        <input type="checkbox" checked={devView} onChange={(e) => setDevView(e.target.checked)} />
        <span>{t("dev.toggle")}</span>
      </label>
    </div>
  );
}
