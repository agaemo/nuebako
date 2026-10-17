import type { AppStatus } from "../../types";

export interface StatusMessage {
  /** 同じ原因の文を二重に出さないための識別子 */
  key: string;
  text: string;
  /** 補足(エラーの理由など)。等幅で小さく出す */
  detail?: string;
}

/** 鍵や保存の問題で一覧を読み込めないときの理由。 */
const STORAGE_MESSAGES: Record<string, string> = {
  key_unavailable:
    "暗号鍵を取得できないため、履歴を読み込んでいません。保存済みの履歴は変更していません",
  key_mismatch:
    "暗号鍵が保存済みの履歴と一致しないため、履歴を読み込んでいません。保存済みの履歴は変更していません",
  error: "履歴を読み込めませんでした。保存済みの履歴は変更していません",
};

export const NO_PERMISSION_MESSAGE = "自動貼り付けには権限が必要です(再コピーのみ)";

/** 一覧を表示できない状態か(鍵が使えない、保存先が壊れているなど)。 */
export const isStorageBad = (status: AppStatus | null): boolean =>
  status !== null && status.storage !== "ok";

interface Inputs {
  status: AppStatus | null;
  /** 自動貼り付けの権限が無く、再コピーのみになる */
  noPermission: boolean;
  /** 一覧の更新に失敗した。表示中の一覧は古い可能性がある */
  listStale: boolean;
}

/** 一覧の上の帯に出す、いまの状態の説明。原因ごとに1文で、重複させない。 */
export function statusMessages({ status, noPermission, listStale }: Inputs): StatusMessage[] {
  const messages: StatusMessage[] = [];
  if (status && status.storage !== "ok") {
    messages.push({
      key: `storage:${status.storage}`,
      text: STORAGE_MESSAGES[status.storage] ?? STORAGE_MESSAGES.error,
      detail: status.storage_message ?? undefined,
    });
  }
  if (status?.capture === "stopped") {
    messages.push({
      key: "capture-stopped",
      text: "クリップボードの記録を停止しています。nuebako を再起動すると再開します",
    });
  }
  if (status?.settings_recovered) {
    messages.push({
      key: "settings-recovered",
      text: "設定ファイルを読み込めなかったため、既定の設定で起動しました",
    });
  }
  if (noPermission) {
    messages.push({ key: "no-permission", text: NO_PERMISSION_MESSAGE });
  }
  if (listStale && !isStorageBad(status)) {
    messages.push({
      key: "list-stale",
      text: "履歴の更新に失敗しました。表示は古い可能性があります",
    });
  }
  return messages;
}
