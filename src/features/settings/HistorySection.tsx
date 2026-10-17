import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/ui/Button";
import { ConfirmDialog } from "../../components/ui/ConfirmDialog";
import { clearHistory, listClips, previewLimits, toAppError } from "../../lib/ipc";
import type { Settings } from "../../types";
import { SettingRow } from "./SettingRow";
import type { UpdateResult } from "./useSettings";

interface HistorySectionProps {
  settings: Settings;
  update: (patch: { max_items: string; retention_days: string }) => Promise<UpdateResult | null>;
}

type Pending = { kind: "limits"; willDelete: number } | { kind: "clear"; total: number };

const inputClass =
  "w-24 rounded-md border border-line bg-surface px-2 py-0.5 text-right font-mono aria-invalid:border-error";

/** 履歴の上限(件数・期間)の変更と、全履歴の削除。破壊的な操作の前には確認ダイアログを出す。 */
export function HistorySection({ settings, update }: HistorySectionProps) {
  const [maxText, setMaxText] = useState(String(settings.max_items));
  const [daysText, setDaysText] = useState(String(settings.retention_days));
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [message, setMessage] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [working, setWorking] = useState(false);
  const reqIdRef = useRef(0);

  // 他の経路で設定が変わったときは、入力欄を最新の値に合わせる
  useEffect(() => {
    setMaxText(String(settings.max_items));
    setDaysText(String(settings.retention_days));
  }, [settings.max_items, settings.retention_days]);

  /** 検証済みの上限を保存する。 */
  const apply = async () => {
    const result = await update({ max_items: maxText, retention_days: daysText });
    if (result === null) return;
    if (result.ok) {
      setErrors({});
      setMessage({ kind: "ok", text: "保存しました" });
    } else if (result.error.code === "validation" && result.error.field) {
      setErrors({ [result.error.field]: result.error.message });
    } else {
      setMessage({ kind: "error", text: result.error.message });
    }
  };

  const save = async () => {
    if (working) return;
    setWorking(true);
    setMessage(null);
    try {
      const preview = await previewLimits(maxText, daysText);
      if (!preview.valid) {
        // 入力エラーのときは、何も変更しない
        setErrors(preview.error ? { [preview.error.field]: preview.error.message } : {});
        return;
      }
      setErrors({});
      if (preview.will_delete > 0) {
        setPending({ kind: "limits", willDelete: preview.will_delete });
        return;
      }
      await apply();
    } catch (e) {
      setMessage({ kind: "error", text: toAppError(e).message });
    } finally {
      setWorking(false);
    }
  };

  const askClear = async () => {
    if (working) return;
    setWorking(true);
    setMessage(null);
    try {
      // 件数だけが欲しいので 1 件だけ取得する(全行とサムネイルを IPC に通さない)
      const page = await listClips("", ++reqIdRef.current, 1, 0);
      if (page.total === 0) {
        setMessage({ kind: "ok", text: "削除する履歴はありません" });
      } else {
        setPending({ kind: "clear", total: page.total });
      }
    } catch (e) {
      setMessage({ kind: "error", text: toAppError(e).message });
    } finally {
      setWorking(false);
    }
  };

  const confirm = async () => {
    if (working || !pending) return;
    setWorking(true);
    try {
      if (pending.kind === "limits") {
        await apply();
      } else {
        const n = await clearHistory();
        setMessage({ kind: "ok", text: `${n.toLocaleString("ja-JP")}件の履歴を削除しました` });
      }
    } catch (e) {
      setMessage({ kind: "error", text: toAppError(e).message });
    } finally {
      setWorking(false);
      setPending(null);
    }
  };

  return (
    <>
      <SettingRow
        label="最大件数"
        hint="超えた分は古い履歴から削除します"
        htmlFor="max-items"
        error={errors.max_items}
      >
        <input
          id="max-items"
          type="text"
          inputMode="numeric"
          value={maxText}
          aria-invalid={errors.max_items ? true : undefined}
          onChange={(e) => setMaxText(e.target.value)}
          className={inputClass}
        />
      </SettingRow>
      <SettingRow
        label="保存期間(日)"
        hint="過ぎた履歴は自動で削除します"
        htmlFor="retention-days"
        error={errors.retention_days}
      >
        <input
          id="retention-days"
          type="text"
          inputMode="numeric"
          value={daysText}
          aria-invalid={errors.retention_days ? true : undefined}
          onChange={(e) => setDaysText(e.target.value)}
          className={inputClass}
        />
      </SettingRow>
      <div className="flex items-center justify-between gap-3">
        <Button variant="danger" busy={working} onClick={askClear}>
          全履歴を削除
        </Button>
        <Button variant="primary" busy={working} onClick={save}>
          保存
        </Button>
      </div>
      {message && (
        <p
          role={message.kind === "error" ? "alert" : "status"}
          className={`m-0 text-[11px] ${message.kind === "error" ? "text-error" : "text-success"}`}
        >
          {message.text}
        </p>
      )}

      {pending?.kind === "limits" && (
        <ConfirmDialog
          title="古い履歴を削除します"
          message={`上限を超える ${pending.willDelete.toLocaleString("ja-JP")} 件の古い履歴が削除されます。元に戻せません。`}
          confirmLabel="削除して保存"
          busy={working}
          onConfirm={confirm}
          onCancel={() => setPending(null)}
        />
      )}
      {pending?.kind === "clear" && (
        <ConfirmDialog
          title="全履歴を削除"
          message={`${pending.total.toLocaleString("ja-JP")}件の履歴をすべて削除します。元に戻せません。`}
          confirmLabel="削除"
          busy={working}
          onConfirm={confirm}
          onCancel={() => setPending(null)}
        />
      )}
    </>
  );
}
