// 一条极简的「已装列表变了」广播。
//
// 为什么需要：列表页、背包页、抽屉、一键升级各有各的刷新时机，
// 但在 A 处装上了、B 处正开着就看不到——以前只有「切回该页时重读」这一层保险。
//
// 三条使用规矩（加新入口的人看这里）：
// 1) 谁该发：任何会改变「已装列表」的代码路径，成功之后发一次。
//    现在会发的是——队列跑完（useQueue，装/卸/导入批量都走它）、今天页一键升级成功、
//    背包页自己的导入/卸载/删空记账、设置页体检里「只删记录」这类修完就变列表的动作、
//    以及单条 install_mod 成功（useSearch.doInstall）。
// 2) 谁该订阅：任何展示「已装了什么」的地方。现在只有 useInstalled（背包页与矿脉页的已装标记共用它）。
// 3) 漏发会怎样：那一页会停在上一次的数据，直到用户切走再切回来或手动点刷新——
//    这正是 v0.1.8 修掉的「背包开着、别处装完它不动」；漏一个调用点就等于这个 bug 复发。
//    多发不要紧（订阅端做了合并），少发才会出问题。
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
