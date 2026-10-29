// 外壳：左边导航，右边当前页。
//
// 三个页都一直挂着，只用 `hidden` 切换显示——这样切回矿脉时搜到的结果、滚动位置、
// 勾选都还在，也不会因为切页把后台任务打断。
import { useEffect, useRef, useState } from "react";
import { ApiError, call, type GameCheck } from "./api";
import AboutDialog from "./components/AboutDialog";
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

/** 切页淡入的时间窗：动画结束会提前摘类，这个只用来兜底（减少动画时没有 animationend） */
const PAGE_IN_MS = 400;

export default function App() {
  const { page, go } = useAppStore();
  const [launchNote, setLaunchNote] = useState("");
  const [aboutOpen, setAboutOpen] = useState(false);
  const [game, setGame] = useState<GameCheck | null>(null);
  const [entering, setEntering] = useState<PageKey | null>(null);
  const asked = useRef(false);

  // 切页时给新页加一次 anim-in，动画放完就摘掉。
  // 三页是靠 `hidden` 切换的，display:none 下的元素不跑过渡，所以这里只能用 keyframes。
  useEffect(() => {
    setEntering(page);
    const timer = setTimeout(() => setEntering(null), PAGE_IN_MS);
    return () => clearTimeout(timer);
  }, [page]);

  // 启动时问一次「游戏本体还是上次那一版吗」：更新过就提示重建，别拿旧包硬塞
  useEffect(() => {
    if (asked.current) return;
    asked.current = true;
    void call<GameCheck>("game_status")
      .then(setGame)
      .catch(() => setGame(null));
  }, []);

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
          <div className="rail-actions">
            <button className="btn ghost small" onClick={() => void launch()}>
              {t("action.launch")}
            </button>
            <button className="btn ghost small" onClick={() => setAboutOpen(true)}>
              {t("nav.about")}
            </button>
          </div>
          {launchNote && <p className="dim small">{launchNote}</p>}
        </div>
      </aside>

      {aboutOpen && <AboutDialog onClose={() => setAboutOpen(false)} />}

      <main className="stage">
        {(game?.changed || (game && !game.has_game)) && (
          <div className="banner warn">
            <span>
              {game.changed ? t("game.changed") : t("game.noGame")}
              {game.changed && game.summary ? `（${game.summary}）` : ""}
            </span>
          </div>
        )}
        <div
          data-page="mine"
          className={entering === "mine" ? "anim-in" : undefined}
          onAnimationEnd={(e) => {
            if (e.target === e.currentTarget) setEntering(null);
          }}
          hidden={page !== "mine"}
        >
          <MineVein onGoSettings={() => go("settings")} />
        </div>
        <div
          data-page="bag"
          className={entering === "bag" ? "anim-in" : undefined}
          onAnimationEnd={(e) => {
            if (e.target === e.currentTarget) setEntering(null);
          }}
          hidden={page !== "bag"}
        >
          <Backpack onGoMine={() => go("mine")} />
        </div>
        <div
          data-page="settings"
          className={entering === "settings" ? "anim-in" : undefined}
          onAnimationEnd={(e) => {
            if (e.target === e.currentTarget) setEntering(null);
          }}
          hidden={page !== "settings"}
        >
          <Settings />
        </div>
      </main>
    </div>
  );
}
