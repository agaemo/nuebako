import type { ClipKind } from "../../types";

/** 種類ごとの表示名と色。色だけに頼らないよう、名前を title とアクセシブル名に付ける。 */
export const KIND_LABELS: Record<ClipKind, string> = {
  text: "テキスト",
  url: "URL",
  code: "コード",
  path: "ファイルパス",
  image: "画像",
};

const KIND_COLORS: Record<ClipKind, string> = {
  text: "bg-kind-text",
  url: "bg-kind-url",
  code: "bg-kind-code",
  path: "bg-kind-path",
  image: "bg-kind-image",
};

/** 行の左端に置く、幅 3px の種類の印。 */
export function KindMark({ kind }: { kind: ClipKind }) {
  return (
    <span
      role="img"
      aria-label={KIND_LABELS[kind]}
      title={KIND_LABELS[kind]}
      className={`inline-block h-full w-[3px] shrink-0 rounded-[1px] ${KIND_COLORS[kind]}`}
    />
  );
}
