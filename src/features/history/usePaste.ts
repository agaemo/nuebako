import { useCallback, useEffect, useRef, useState } from "react";
import { getAppStatus, openAccessibilitySettings, pasteClip, updateSettings } from "../../lib/ipc";
import { onHistoryWindowShown } from "../../lib/events";
import { unlistenAll } from "./unlisten";
import type { AppStatus, PasteMode } from "../../types";

/**
 * 貼り付けの操作。権限の判定と再コピーの手順は Rust の paste_clip が担う。
 * ここでは、初回(権限なし・案内未表示)の案内(S4)と、結果に応じたフッター案内だけを扱う。
 */
export function usePaste() {
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [guideFor, setGuideFor] = useState<number | null>(null);
  const [lastReason, setLastReason] = useState<"no_permission" | "send_failed" | null>(null);

  const busyRef = useRef(false);
  const statusRef = useRef<AppStatus | null>(null);
  const guideForRef = useRef<number | null>(null);

  const loadStatus = useCallback(async () => {
    try {
      const s = await getAppStatus();
      statusRef.current = s;
      setStatus(s);
    } catch {
      // 状態が取れなくても貼り付け自体は試せる
    }
  }, []);

  useEffect(() => {
    void loadStatus();
    const un = onHistoryWindowShown(() => {
      setLastReason(null);
      void loadStatus();
    });
    return () => unlistenAll([un]);
  }, [loadStatus]);

  /** 貼り付けの実行。実行中の呼び出し(Enter 連打)は無視する。 */
  const run = useCallback(async (id: number, mode: PasteMode) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    try {
      const outcome = await pasteClip(id, mode);
      setLastReason(outcome.result === "copied_only" ? outcome.reason : null);
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

  const closeGuide = useCallback(() => {
    guideForRef.current = null;
    setGuideFor(null);
  }, []);

  /** 案内の「あとで(再コピーのみ)」。非表示を保存して、再コピーのみ実行する。 */
  const guideLater = useCallback(async () => {
    const id = guideForRef.current;
    if (id === null || busyRef.current) return;
    try {
      await updateSettings({ permission_guide_dismissed: true });
      if (statusRef.current) {
        statusRef.current = { ...statusRef.current, permission_guide_dismissed: true };
        setStatus(statusRef.current);
      }
    } catch {
      // 保存に失敗しても、再コピーは実行する
    }
    closeGuide();
    await run(id, "copy_only");
  }, [closeGuide, run]);

  const openSystemSettings = useCallback(async () => {
    await openAccessibilitySettings();
  }, []);

  const noPermissionNote =
    lastReason === "no_permission" ||
    (status !== null && !status.paste_permission && status.permission_guide_dismissed);

  return {
    status,
    statusLoaded: status !== null,
    busy,
    guideFor,
    noPermissionNote,
    sendFailedNote: lastReason === "send_failed",
    paste,
    closeGuide,
    guideLater,
    openSystemSettings,
    reloadStatus: loadStatus,
  };
}
