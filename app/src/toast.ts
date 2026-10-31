// 轻提示：底部一行字，2 秒自己消失。
//
// 为什么用 window 事件而不是状态树：只有「点链接」这一处在用，没必要为它
// 往 appStore 里加一个字段、让所有页面跟着重渲染。

const EVENT = "rs:toast";

/** 发一条提示。同一条文字连着发两次也会重新计时。 */
export function showToast(text: string) {
  window.dispatchEvent(new CustomEvent<string>(EVENT, { detail: text }));
}

/** 订阅提示，返回取消订阅的函数。 */
export function onToast(fn: (text: string) => void) {
  const handler = (event: Event) => fn((event as CustomEvent<string>).detail);
  window.addEventListener(EVENT, handler);
  return () => window.removeEventListener(EVENT, handler);
}
