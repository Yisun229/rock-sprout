// 往剪贴板写东西，成功返回 true。
//
// 窗口没在前台时系统可能一直不给答复（不是报错，是干脆不返回），
// 所以这里加个上限：超时就当「没写成」，让界面叫用户自己选中复制，别把按钮卡在「忙」上。
export async function writeClipboard(text: string): Promise<boolean> {
  try {
    await Promise.race([
      navigator.clipboard.writeText(text),
      new Promise((_, reject) => setTimeout(() => reject(new Error("超时")), 1200)),
    ]);
    return true;
  } catch {
    return false;
  }
}
