import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "../../components/ui/Button";
import { getAppStatus, setShortcut, toAppError } from "../../lib/ipc";
import type { AppRef, SettingsPatch, Theme } from "../../types";
import { AppExcludeList } from "./AppExcludeList";
import { CollapsibleSection } from "./CollapsibleSection";
import { HistorySection } from "./HistorySection";
import { SettingRow } from "./SettingRow";
import { ShortcutInput } from "./ShortcutInput";
import { useSettings, type SaveResult } from "./useSettings";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "システムに合わせる" },
  { value: "light", label: "ライト" },
  { value: "dark", label: "ダーク" },
];

/** 設定ウィンドウを隠す(常駐のため破棄しない)。権限が無いなどで失敗しても画面は壊さない。 */
async function hideSelf() {
  try {
    await getCurrentWindow().hide();
  } catch {
    // 閉じられないときは何もしない
  }
}

/** 設定ウィンドウ。Esc または閉じるボタンで隠す。 */
export function SettingsWindow() {
  const { settings, loadError, update, replace } = useSettings();
  const [version, setVersion] = useState<string | null>(null);
  const [themeError, setThemeError] = useState<string | null>(null);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") void hideSelf();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    let cancelled = false;
    getAppStatus()
      .then((s) => !cancelled && setVersion(s.version))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  /** 更新して結果を返す。別の保存が処理中のときは skipped で、呼び出し側は表示を変えない。 */
  const save = async (patch: SettingsPatch): Promise<SaveResult> => {
    const result = await update(patch);
    if (result === null) return { status: "skipped" };
    return result.ok ? { status: "saved" } : { status: "error", message: result.error.message };
  };

  const commitShortcut = async (accelerator: string): Promise<string | null> => {
    try {
      replace(await setShortcut(accelerator));
      return null;
    } catch (e) {
      const err = toAppError(e);
      return err.code === "shortcut_conflict"
        ? `${err.message}。元の設定を維持しています`
        : err.message;
    }
  };

  return (
    <div className="w-[420px] max-w-full">
      <header className="flex items-center justify-between border-b border-line px-4 py-2">
        <h1 className="m-0 font-display text-[14px] font-bold">設定</h1>
        <Button variant="ghost" aria-label="閉じる" onClick={() => void hideSelf()}>
          閉じる
        </Button>
      </header>

      {loadError && (
        <p role="alert" className="m-4 text-error">
          設定を読み込めません: {loadError.message}
        </p>
      )}
      {!settings && !loadError && <p className="m-4 text-text-sub">読み込み中…</p>}

      {settings && (
        <>
          <CollapsibleSection title="一般" defaultOpen>
            <SettingRow
              label="呼び出しショートカット"
              hint="クリックしてから、新しいキーを押します"
            >
              <ShortcutInput value={settings.shortcut} onCommit={commitShortcut} />
            </SettingRow>
            <SettingRow label="テーマ" htmlFor="theme" error={themeError}>
              <select
                id="theme"
                value={settings.theme}
                onChange={async (e) => {
                  const result = await save({ theme: e.target.value as Theme });
                  if (result.status !== "skipped") {
                    setThemeError(result.status === "error" ? result.message : null);
                  }
                }}
                className="rounded-md border border-line bg-surface px-2 py-0.5"
              >
                {THEMES.map((t) => (
                  <option key={t.value} value={t.value}>
                    {t.label}
                  </option>
                ))}
              </select>
            </SettingRow>
          </CollapsibleSection>

          <CollapsibleSection title="履歴">
            <HistorySection settings={settings} update={update} />
          </CollapsibleSection>

          <CollapsibleSection title="除外アプリ">
            <AppExcludeList
              apps={settings.excluded_apps}
              recordUnknownSource={settings.record_unknown_source}
              onChangeApps={(apps: AppRef[]) => save({ excluded_apps: apps })}
              onChangeRecordUnknown={(v) => save({ record_unknown_source: v })}
            />
          </CollapsibleSection>

          <CollapsibleSection title="このアプリについて">
            <p className="m-0">バージョン: {version ?? "不明"}</p>
            <p className="m-0">ライセンス: MIT</p>
          </CollapsibleSection>
        </>
      )}
    </div>
  );
}
