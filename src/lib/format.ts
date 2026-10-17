// 表示用の整形。保存は UTC の Unix ミリ秒で、表示はローカル時刻にする。

const pad2 = (n: number) => String(n).padStart(2, "0");

/**
 * 履歴の時刻表示。ローカル時刻で「今日」なら HH:mm、それ以外は M/D。
 * now は呼び出し側が渡す(履歴ウィンドウは、一覧の応答を反映した時刻で全行をまとめて描画する。
 * 取り直しのたびに更新されるので、開きっぱなしで日付が変わっても追従する)。省略時は現在時刻。
 * 時計が戻って未来になったコピーも、日付が違えば M/D で表示する。
 */
export function formatClipTime(copiedAtMs: number, nowMs: number = Date.now()): string {
  const d = new Date(copiedAtMs);
  const now = new Date(nowMs);
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  if (sameDay) return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/** プレビュー用の日時(YYYY/M/D HH:mm)。 */
export function formatClipDateTime(copiedAtMs: number): string {
  const d = new Date(copiedAtMs);
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}
