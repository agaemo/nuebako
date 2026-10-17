import { describe, it } from "vitest";

// 共有ファイル。各トラックが自分のテストを追記する(相手のテストは変更しない)。
describe("keys", () => {
  it.todo("track-settings: toAccelerator のテストを追記する");
  it.todo("track-ui-history: 番号貼り付けの解釈のテストを追記する");
});
