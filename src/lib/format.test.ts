import { afterEach, describe, expect, it, vi } from "vitest";
import { formatClipTime } from "./format";

/** ローカル時刻の Unix ミリ秒 */
const at = (y: number, mo: number, d: number, h = 0, mi = 0) =>
  new Date(y, mo - 1, d, h, mi).getTime();

afterEach(() => {
  vi.useRealTimers();
});

describe("formatClipTime", () => {
  it("同じ日のとき HH:mm で表示する", () => {
    expect(formatClipTime(at(2026, 5, 10, 9, 5), at(2026, 5, 10, 18, 30))).toBe("09:05");
  });

  it("前の日のとき M/D で表示する", () => {
    expect(formatClipTime(at(2026, 5, 9, 23, 59), at(2026, 5, 10, 0, 1))).toBe("5/9");
  });

  it("0時をまたぐと、同じコピーの表示が HH:mm から M/D に変わる", () => {
    const copied = at(2026, 3, 1, 23, 59);
    expect(formatClipTime(copied, at(2026, 3, 1, 23, 59))).toBe("23:59");
    expect(formatClipTime(copied, at(2026, 3, 2, 0, 0))).toBe("3/1");
  });

  it("月末から翌月に変わると、月末の日付を M/D で表示する", () => {
    expect(formatClipTime(at(2026, 1, 31, 10, 0), at(2026, 2, 1, 10, 0))).toBe("1/31");
  });

  it("うるう日 2/29 を M/D で表示する", () => {
    expect(formatClipTime(at(2028, 2, 29, 12, 0), at(2028, 3, 1, 8, 0))).toBe("2/29");
  });

  it("うるう年でない年は 2/28 の翌日が 3/1 になり、2/28 は M/D で表示する", () => {
    expect(formatClipTime(at(2027, 2, 28, 12, 0), at(2027, 3, 1, 8, 0))).toBe("2/28");
  });

  it("うるう日の当日は HH:mm で表示する", () => {
    expect(formatClipTime(at(2028, 2, 29, 7, 3), at(2028, 2, 29, 23, 0))).toBe("07:03");
  });

  it("時計が戻って未来になった日付は M/D で表示する", () => {
    expect(formatClipTime(at(2026, 5, 12, 10, 0), at(2026, 5, 10, 10, 0))).toBe("5/12");
  });

  it("now を省略すると現在時刻(固定した時計)を使い、呼ぶたびに取り直す", () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    const copied = at(2026, 3, 1, 23, 59);
    vi.setSystemTime(at(2026, 3, 1, 23, 59));
    expect(formatClipTime(copied)).toBe("23:59");
    vi.setSystemTime(at(2026, 3, 2, 0, 0));
    expect(formatClipTime(copied)).toBe("3/1");
  });
});
