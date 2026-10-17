import { useCallback, useEffect, useRef, useState } from "react";
import { onSettingsChanged } from "../../lib/events";
import { getSettings, toAppError, updateSettings } from "../../lib/ipc";
import { applyTheme } from "../../lib/theme";
import type { AppError, Settings, SettingsPatch } from "../../types";

export type UpdateResult = { ok: true; settings: Settings } | { ok: false; error: AppError };

/**
 * 設定の読み込み・更新・購読。更新の実行中は busy になり、二重実行しない。
 * テーマは設定が変わるたびに反映する。
 */
export function useSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loadError, setLoadError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    getSettings()
      .then((s) => !cancelled && setSettings(s))
      .catch((e) => !cancelled && setLoadError(toAppError(e)));
    onSettingsChanged((s) => !cancelled && setSettings(s))
      .then((u) => {
        if (cancelled) u();
        else unlisten = u;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      try {
        unlisten?.();
      } catch {
        // 購読の解除に失敗しても画面には影響しない
      }
    };
  }, []);

  useEffect(() => {
    if (settings) applyTheme(settings.theme);
  }, [settings]);

  /** 設定を更新する。実行中に呼ばれたときは何もしない(ref で判定し、await をまたいで state を読まない)。 */
  const update = useCallback(async (patch: SettingsPatch): Promise<UpdateResult | null> => {
    if (busyRef.current) return null;
    busyRef.current = true;
    setBusy(true);
    try {
      const next = await updateSettings(patch);
      setSettings(next);
      return { ok: true, settings: next };
    } catch (e) {
      return { ok: false, error: toAppError(e) };
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }, []);

  /** 他の経路(ショートカット変更など)で得た最新の設定を反映する。 */
  const replace = useCallback((next: Settings) => setSettings(next), []);

  return { settings, loadError, busy, update, replace };
}
