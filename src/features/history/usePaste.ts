import { useCallback, useEffect, useRef, useState } from "react";
import { getAppStatus, openAccessibilitySettings, pasteClip, updateSettings } from "../../lib/ipc";
import { onHistoryWindowShown } from "../../lib/events";
import { unlistenAll } from "./unlisten";
import type { AppStatus, PasteMode, PasteReason } from "../../types";

interface UsePasteOptions {
  /** 貼り付けの呼び出し自体が失敗したときの通知 */
  onError?: (message: string) => void;
}

/**
 * 貼り付けの操作。権限の判定と再コピーの手順は Rust の paste_clip が担う。
 * ここでは、初回(権限なし・案内未表示)の案内と、結果に応じたフッター案内だけを扱う。
 */
export function usePaste({ onError }: UsePasteOptions = {}) {
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [guideFor, setGuideFor] = useState<number | null>(null);
  const [lastReason, setLastReason] = useState<PasteReason | null>(null);

  const busyRef = useRef(false);
  const statusRef = useRef<AppStatus | null>(null);
  const guideForRef = useRef<number | null>(null);
  // onError は呼び出し側が毎回作り直すので、ref 経由で最新を呼ぶ(run の依存を増やさない)
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;

  const loadStatus = useCallback(async () => {
    try {
      const s = await getAppStatus();
      statusRef.current = s;
      setStatus(s);
    } catch {
      // 状態が取れなくても貼り付け自体は試せる
    }
  }, []);

  const closeGuide = useCallback(() => {
    guideForRef.current = null;
    setGuideFor(null);
  }, []);

  useEffect(() => {
    void loadStatus();
    // 非表示の間に権限を変えている場合があるので、案内は閉じて、権限を判定し直す
    const un = onHistoryWindowShown(() => {
      setLastReason(null);
      closeGuide();
      void loadStatus();
    });
    return () => unlistenAll([un]);
  }, [loadStatus, closeGuide]);

  /** 貼り付けの実行。実行中の呼び出し(Enter 連打)は無視する。 */
  const run = useCallback(async (id: number, mode: PasteMode) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    try {
      const outcome = await pasteClip(id, mode);
      setLastReason(outcome.result === "copied_only" ? outcome.reason : null);
    } catch {
      // 失敗の詳細は画面に出さない(貼り付け対象の内容を含みうるため)。失敗した事実だけ知らせる
      onErrorRef.current?.("貼り付けに失敗しました");
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }, []);

  const paste = useCallback(
    async (id: number) => {
      if (busyRef.current || guideForRef.current !== null) return;
      const s = statusRef.current;
      if (s && !s.paste_permission && !s.permission_guide_dismissed) {
        guideForRef.current = id;
        setGuideFor(id);
        return;
      }
      await run(id, "auto");
    },
    [run],
  );

  /**
   * 案内の「あとで(再コピーのみ)」。再コピーのみ実行する。
   * 案内を出してから権限を許可していた場合は、案内を次回も出さない設定にしてしまわないよう、
   * 権限を判定し直して、無いときだけ非表示を保存する。
   */
  const guideLater = useCallback(async () => {
    const id = guideForRef.current;
    if (id === null || busyRef.current) return;
    closeGuide(); // 先に閉じて、連打による二重実行を防ぐ
    await loadStatus();
    const current = statusRef.current;
    if (current && !current.paste_permission) {
      try {
        await updateSettings({ permission_guide_dismissed: true });
        statusRef.current = { ...current, permission_guide_dismissed: true };
        setStatus(statusRef.current);
      } catch {
        // 保存に失敗しても、再コピーは実行する
      }
    }
    await run(id, "copy_only");
  }, [closeGuide, loadStatus, run]);

  const openSystemSettings = useCallback(async () => {
    await openAccessibilitySettings();
  }, []);

  const noPermissionNote =
    lastReason === "no_permission" ||
    (status !== null && !status.paste_permission && status.permission_guide_dismissed);

  return {
    status,
    busy,
    guideFor,
    noPermissionNote,
    sendFailedNote: lastReason === "send_failed",
    focusLostNote: lastReason === "focus_lost",
    paste,
    closeGuide,
    guideLater,
    openSystemSettings,
  };
}
