// 出错时统一长这样：一句「哪里出了问题」+ 一句「你现在该做什么」+ 可点的动作。
// 三种最常见的情况（没令牌 / 连不上 / 被限速）各有各的说法，别让用户看同一句「请求失败」。
import type { ApiError } from "./api";

type Props = {
  error: ApiError;
  onRetry?: () => void;
  onGoSettings?: () => void;
};

const TIPS: Record<string, { title: string; advice: string; settings: boolean }> = {
  no_token: {
    title: "还没填访问令牌",
    advice: "mod.io 要认人。去设置页把属于你的令牌粘进来，只用粘一次。",
    settings: true,
  },
  unauthorized: {
    title: "令牌无效或已经过期",
    advice: "到 mod.io 的账号页重新生成一串，再贴到设置页。",
    settings: true,
  },
  rate_limited: {
    title: "mod.io 让你先等一会儿",
    advice: "短时间内请求太多。过几分钟再点，不用改设置。",
    settings: false,
  },
  network: {
    title: "连不上 mod.io",
    advice: "先确认网是通的。这台机器平时访问不了 mod.io，得先开自己的加速工具。",
    settings: false,
  },
  integrate: {
    title: "装进游戏这一步出错了",
    advice: "如果游戏正开着，先退掉再试——游戏目录里的文件被占用时改不动。",
    settings: false,
  },
  not_installed: {
    title: "这个 mod 不在已装列表里",
    advice: "可能是已经卸过了。刷新一下列表看看。",
    settings: false,
  },
};

export default function ErrorHint({ error, onRetry, onGoSettings }: Props) {
  const tip = TIPS[error.code] ?? {
    title: "出错了",
    advice: "把下面这句话记下来，方便排查。",
    settings: false,
  };

  return (
    <div className="card fail">
      <div className="card-title fail-title">{tip.title}</div>
      <p className="note">{tip.advice}</p>
      <p className="note small dim">原始信息：{error.message}</p>
      <div className="searchbar">
        {onRetry && (
          <button className="btn small" onClick={onRetry}>
            重试
          </button>
        )}
        {tip.settings && onGoSettings && (
          <button className="btn ghost small" onClick={onGoSettings}>
            去设置页
          </button>
        )}
      </div>
    </div>
  );
}
