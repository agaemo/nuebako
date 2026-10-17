import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Toggle } from "../../components/ui/Toggle";
import { listRunningApps, toAppError } from "../../lib/ipc";
import type { AppRef } from "../../types";
import { SettingRow } from "./SettingRow";
import type { SaveResult } from "./useSettings";

interface AppExcludeListProps {
  apps: AppRef[];
  recordUnknownSource: boolean;
  /** 変更の保存。結果(保存した、スキップした、失敗した)を返す */
  onChangeApps: (apps: AppRef[]) => Promise<SaveResult>;
  onChangeRecordUnknown: (value: boolean) => Promise<SaveResult>;
}

/** 除外アプリの一覧、実行中のアプリからの追加、コピー元が不明なときの記録の設定。 */
export function AppExcludeList({
  apps,
  recordUnknownSource,
  onChangeApps,
  onChangeRecordUnknown,
}: AppExcludeListProps) {
  const [candidates, setCandidates] = useState<AppRef[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** 保存の結果をエラー表示に反映する。スキップ(別の保存が処理中)のときは表示を変えない。 */
  const run = async (task: Promise<SaveResult>): Promise<SaveResult> => {
    const result = await task;
    if (result.status !== "skipped") setError(result.status === "error" ? result.message : null);
    return result;
  };

  const openPicker = async () => {
    if (loading) return;
    setLoading(true);
    setError(null);
    try {
      setCandidates(await listRunningApps());
    } catch (e) {
      setError(toAppError(e).message);
    } finally {
      setLoading(false);
    }
  };

  const available = (candidates ?? []).filter((c) => !apps.some((a) => a.id === c.id));

  return (
    <div className="flex flex-col gap-3">
      {apps.length === 0 ? (
        <p className="m-0 text-text-sub">除外するアプリはありません</p>
      ) : (
        <ul aria-label="除外アプリ" className="m-0 flex list-none flex-col gap-1 p-0">
          {apps.map((app) => (
            <li
              key={app.id}
              className="flex items-center justify-between gap-2 rounded-md border border-line bg-surface px-2 py-1"
            >
              <span className="min-w-0 truncate">{app.name}</span>
              <Button
                aria-label={`${app.name}を除外から外す`}
                variant="ghost"
                onClick={() => run(onChangeApps(apps.filter((a) => a.id !== app.id)))}
              >
                削除
              </Button>
            </li>
          ))}
        </ul>
      )}

      <div>
        <Button busy={loading} onClick={openPicker}>
          実行中のアプリから追加
        </Button>
      </div>

      {candidates !== null && (
        <div className="rounded-md border border-line bg-surface p-2">
          {available.length === 0 ? (
            <p className="m-0 text-text-sub">追加できるアプリはありません</p>
          ) : (
            <ul
              aria-label="実行中のアプリ"
              className="m-0 flex max-h-40 list-none flex-col gap-1 overflow-y-auto p-0"
            >
              {available.map((app) => (
                <li key={app.id}>
                  <button
                    type="button"
                    onClick={async () => {
                      const result = await run(onChangeApps([...apps, app]));
                      // 保存できたときだけ候補から外す(スキップや失敗では、あとで選び直せるよう残す)
                      if (result.status === "saved") {
                        setCandidates((prev) => prev && prev.filter((c) => c.id !== app.id));
                      }
                    }}
                    className="w-full rounded-sm px-2 py-0.5 text-left hover:bg-select"
                  >
                    {app.name}
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div className="mt-2 text-right">
            <Button variant="ghost" onClick={() => setCandidates(null)}>
              閉じる
            </Button>
          </div>
        </div>
      )}

      <SettingRow
        label="コピー元が不明なときも記録する"
        hint="オフにすると、コピー元を判別できないコピーは記録しません"
      >
        <Toggle
          label="コピー元が不明なときも記録する"
          checked={recordUnknownSource}
          onChange={(v) => run(onChangeRecordUnknown(v))}
        />
      </SettingRow>

      {error && (
        <p role="alert" className="m-0 text-[11px] text-error">
          {error}
        </p>
      )}
    </div>
  );
}
