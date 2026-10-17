import { useEffect } from "react";
import { onSettingsChanged } from "./events";
import { getSettings } from "./ipc";
import { applyTheme } from "./theme";

/**
 * 設定のテーマをこのウィンドウに反映する。ウィンドウごとに別の WebView なので、
 * 設定を変えた側だけでなく、履歴ウィンドウも自分で読み込んで購読する必要がある。
 */
export function useTheme(): void {
  useEffect(() => {
    let cancelled = false;
    // 変更の通知が先に届いたときは、遅れて返った古い読み込み結果で上書きしない
    let notified = false;
    getSettings()
      .then((s) => {
        if (!cancelled && !notified && s) applyTheme(s.theme);
      })
      .catch(() => {
        // 読めないときは OS の設定のままにする
      });
    const unlisten = onSettingsChanged((s) => {
      notified = true;
      applyTheme(s.theme);
    });
    return () => {
      cancelled = true;
      void unlisten.then((f) => f()).catch(() => {});
    };
  }, []);
}
