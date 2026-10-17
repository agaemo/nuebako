/** イベント購読の解除。解除の失敗(ウィンドウ破棄後など)は無視する。 */
export function unlistenAll(subscriptions: Array<Promise<() => void>>): void {
  for (const p of subscriptions) {
    void p.then((f) => f()).catch(() => {});
  }
}
