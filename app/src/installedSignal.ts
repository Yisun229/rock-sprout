// 一条极简的「已装列表变了」广播。
//
// 为什么需要：列表页、背包页、抽屉、一键升级各有各的刷新时机，
// 但在 A 处装上了、B 处正开着就看不到——以前只有「切回该页时重读」这一层保险。
// 这里把「装完 / 卸完 / 升级完」汇成一个事件，谁关心谁自己听。
const EVENT = "rs:installed";

/** 装、卸、升级之后调一下；调用方不用关心有没有人在听 */
export function notifyInstalled(): void {
  window.dispatchEvent(new Event(EVENT));
}

/** 订阅；返回退订函数 */
export function onInstalled(fn: () => void): () => void {
  const handler = () => fn();
  window.addEventListener(EVENT, handler);
  return () => window.removeEventListener(EVENT, handler);
}
