import { useState } from "react";
import { ApiError, call } from "./api";
import MineVein from "./pages/MineVein";
import Backpack from "./pages/Backpack";
import Settings from "./pages/Settings";

/** 左侧「矿层」组：装 mod 相关的主流程。 */
const MINE_PAGES = [
  { key: "mine", label: "矿脉" },
  { key: "bag", label: "背包" },
  { key: "settings", label: "设置" },
] as const;

type PageKey = (typeof MINE_PAGES)[number]["key"];

export default function App() {
  const [page, setPage] = useState<PageKey>("mine");
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
        <div className="brand">岩笋</div>
        <div className="group-title">矿层</div>
        {MINE_PAGES.map((p) => (
          <button
            key={p.key}
            className={p.key === page ? "nav-item on" : "nav-item"}
            onClick={() => setPage(p.key)}
          >
            {p.label}
          </button>
        ))}

        <div className="rail-foot">
          <button className="btn ghost small" onClick={() => void launch()}>
            启动游戏
          </button>
          {launchNote && <p className="dim small">{launchNote}</p>}
        </div>
      </aside>

      <main className="stage">
        {page === "mine" && <MineVein onGoSettings={() => setPage("settings")} />}
        {page === "bag" && <Backpack />}
        {page === "settings" && <Settings />}
      </main>
    </div>
  );
}
