import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { KIND_LABELS, KindMark } from "./KindMark";
import type { ClipKind } from "../../types";

describe("KindMark", () => {
  it.each(Object.keys(KIND_LABELS) as ClipKind[])(
    "%s のとき、種類の名前がアクセシブル名とtitleになる",
    (kind) => {
      render(<KindMark kind={kind} />);
      const mark = screen.getByRole("img", { name: KIND_LABELS[kind] });
      expect(mark.getAttribute("title")).toBe(KIND_LABELS[kind]);
    },
  );
});
