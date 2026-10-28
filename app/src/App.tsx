// 外壳：左边导航，右边当前页。
//
// 三个页都一直挂着，只用 `hidden` 切换显示——这样切回矿脉时搜到的结果、滚动位置、
// 勾选都还在，也不会因为切页把后台任务打断。
import { useState } from "react";
import { ApiError, call } from "./api";
import { t, type TextKey } from "./i18n";
import Backpack from "./pages/Backpack";
import MineVein from "./pages/MineVein";
import Settings from "./pages/Settings";
import { useAppStore, type PageKey } from "./store/appStore";

const NAV: { key: PageKey; label: TextKey }[] = [
  { key: "mine", label: "nav.mine" },
  { key: "bag", label: "nav.bag" },
  { key: "settings", label: "nav.settings" },
];

export default function App() {
  const { page, go } = useAppStore();
  const [launchNote, setLaunchNote] = useState("");

  async function launch() {
    try {
      const res = await call<{ requested: boolean; message: string }>("launch_game");
      setLaunchNote(res.message);
    } catch (e) {
      setLaunchNote(e instanceof ApiError ? e.message : String(e));
    }
  }

  return (
    <div className="shell">
      <aside className="rail">
        <div className="brand">{t("app.brand")}</div>
        <div className="group-title">{t("nav.group.mine")}</div>
        {NAV.map((p) => (
          <button
            key={p.key}
            className={p.key === page ? "nav-item on" : "nav-item"}
            onClick={() => go(p.key)}
          >
            {t(p.label)}
          </button>
        ))}

        <div className="rail-foot">
          <button className="btn ghost small" onClick={() => void launch()}>
            {t("action.launch")}
          </button>
          {launchNote && <p className="dim small">{launchNote}</p>}
        </div>
      </aside>

      <main className="stage">
        <div data-page="mine" hidden={page !== "mine"}>
          <MineVein onGoSettings={() => go("settings")} />
        </div>
        <div data-page="bag" hidden={page !== "bag"}>
          <Backpack />
        </div>
        <div data-page="settings" hidden={page !== "settings"}>
          <Settings />
        </div>
      </main>
    </div>
  );
}
