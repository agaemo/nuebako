import type { ReactNode } from "react";

interface SettingRowProps {
  label: string;
  /** 補足の説明 */
  hint?: string;
  /** ラベルと対応させる入力欄の id */
  htmlFor?: string;
  /** この項目のエラー表示 */
  error?: string | null;
  children: ReactNode;
}

/** 設定の1項目(左にラベル、右に入力)。 */
export function SettingRow({ label, hint, htmlFor, error, children }: SettingRowProps) {
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <label htmlFor={htmlFor} className="block text-text">
            {label}
          </label>
          {hint && <p className="m-0 text-[11px] text-text-sub">{hint}</p>}
        </div>
        <div className="shrink-0">{children}</div>
      </div>
      {error && (
        <p role="alert" className="m-0 text-[11px] text-error">
          {error}
        </p>
      )}
    </div>
  );
}
