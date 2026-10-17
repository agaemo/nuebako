import type { ClipKind } from "../../types";

/** 履歴ウィンドウの高さの内訳(px)。design.md: 1行 24px、最大10行、最大約 330px、プレビュー +96px。 */
export const ROW_HEIGHT = 24;
export const MAX_ROWS = 10;
export const PREVIEW_HEIGHT = 96;
/** 画像のプレビューの高さ。画像を見やすくするため、テキストより高くする。 */
export const PREVIEW_IMAGE_HEIGHT = 240;
const HEADER = 28;
const SEARCH = 32;
const FOOTER = 26;
/** 0件のときの案内文の領域 */
const EMPTY_AREA = 72;

/** 内容(表示する行数、プレビューの開閉と、選択中の種類)に合わせたウィンドウの高さ。 */
export function windowHeight(
  rowCount: number,
  previewOpen: boolean,
  previewKind?: ClipKind,
): number {
  const rows = Math.min(Math.max(rowCount, 0), MAX_ROWS);
  const list = rows === 0 ? EMPTY_AREA : rows * ROW_HEIGHT;
  return HEADER + SEARCH + list + FOOTER + (previewOpen ? previewHeight(previewKind) : 0);
}

/** プレビューの高さ。画像は高く、それ以外は既定。 */
export function previewHeight(kind?: ClipKind): number {
  return kind === "image" ? PREVIEW_IMAGE_HEIGHT : PREVIEW_HEIGHT;
}
