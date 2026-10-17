import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it } from "vitest";
import type { AppStatus, ClipDetail, ClipSummary, PasteOutcome } from "../../types";
import { HistoryWindow } from "./HistoryWindow";

const clip = (id: number, over: Partial<ClipSummary> = {}): ClipSummary => ({
  id,
  kind: "text",
  first_line: `item-${id}`,
  copied_at_ms: new Date(2026, 4, 10, 9, id).getTime(),
  pinned: false,
  source_app_name: null,
  char_count: 6,
  thumb_png: null,
  ...over,
});

const baseStatus: AppStatus = {
  os: "macos",
  version: "0.1.0",
  storage: "ok",
  storage_message: null,
  paste_permission: true,
  permission_guide_dismissed: false,
};

interface Env {
  items: ClipSummary[];
  status: AppStatus;
  outcome: PasteOutcome;
  details: Record<number, ClipDetail>;
  calls: Array<{ cmd: string; args: unknown }>;
}

function setup(over: Partial<Env> = {}) {
  const env: Env = {
    items: [],
    status: baseStatus,
    outcome: { result: "pasted", reason: null },
    details: {},
    calls: [],
    ...over,
  };
  mockWindows("history");
  mockIPC(
    (cmd, args) => {
      env.calls.push({ cmd, args });
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "get_app_status":
          return env.status;
        case "list_clips": {
          const q = String(a.query ?? "").toLowerCase();
          const hit = env.items.filter((c) => c.first_line.toLowerCase().includes(q));
          return { req_id: a.reqId, total: env.items.length, items: hit };
        }
        case "get_clip_detail":
          return env.details[a.id as number];
        case "paste_clip":
          return env.outcome;
        case "delete_clip":
          env.items = env.items.filter((c) => c.id !== a.id);
          return null;
        case "set_pinned":
          env.items = env.items.map((c) =>
            c.id === a.id ? { ...c, pinned: a.pinned as boolean } : c,
          );
          return null;
        case "update_settings":
          return {};
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  return env;
}

const calls = (env: Env, cmd: string) => env.calls.filter((c) => c.cmd === cmd);
const options = () => screen.queryAllByRole("option");

beforeEach(() => {
  document.body.innerHTML = "";
});

describe("HistoryWindow 一覧", () => {
  it("件数の範囲と全件数をヘッダーに表示する", async () => {
    setup({ items: [clip(1), clip(2), clip(3)] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    expect(screen.getByTestId("count").textContent).toBe("履歴 1〜3 件 / 全 3 件");
  });

  it("最大10行を表示し、番号は 1〜9 と 10行目の 0 になる", async () => {
    const items = Array.from({ length: 12 }, (_, i) => clip(i + 1));
    setup({ items });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(10));
    expect(screen.getByTestId("count").textContent).toBe("履歴 1〜10 件 / 全 12 件");
    const nums = options().map((o) => within(o).getByText(/^[0-9]$/).textContent);
    expect(nums).toEqual(["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]);
  });

  it("各行に種類の印(名前付き)と、本文の先頭1行を表示する", async () => {
    setup({ items: [clip(1, { kind: "url", first_line: "https://example.com" })] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    expect(screen.getByRole("img", { name: "URL" }).getAttribute("title")).toBe("URL");
    expect(screen.getByText("https://example.com")).toBeTruthy();
  });

  it("ピン留めした行は番号の代わりにピン留めの印を表示する", async () => {
    setup({ items: [clip(1, { pinned: true }), clip(2)] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    expect(within(options()[0]).getByRole("img", { name: "ピン留め" })).toBeTruthy();
    expect(within(options()[1]).getByText("2")).toBeTruthy();
  });

  it("画像の行は data URL のサムネイルを表示する", async () => {
    setup({ items: [clip(1, { kind: "image", first_line: "", thumb_png: "AAAA" })] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    const img = options()[0].querySelector("img[src^='data:image/png;base64,']");
    expect(img).not.toBeNull();
  });

  it("履歴の内容を HTML として解釈しない", async () => {
    setup({ items: [clip(1, { first_line: "<b>bold</b><img src=x>" })] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    expect(options()[0].querySelector("b")).toBeNull();
    expect(options()[0].textContent).toContain("<b>bold</b>");
  });

  it("履歴が0件のとき案内文を表示する", async () => {
    setup({ items: [] });
    render(<HistoryWindow />);
    await screen.findByText("まだ履歴がありません。何かコピーすると、ここに並びます");
  });

  it("検索で絞り込み、大文字小文字を区別せず、0件のとき別の案内文を表示する", async () => {
    setup({ items: [clip(1, { first_line: "Hello World" }), clip(2, { first_line: "other" })] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.type(screen.getByRole("searchbox"), "hello");
    await waitFor(() => expect(options().length).toBe(1));
    await user.clear(screen.getByRole("searchbox"));
    await user.type(screen.getByRole("searchbox"), "zzz");
    await screen.findByText("一致する履歴がありません");
  });

  it("表示時に検索欄にフォーカスする", async () => {
    setup({ items: [clip(1)] });
    render(<HistoryWindow />);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("searchbox")));
  });
});

describe("HistoryWindow 時刻", () => {
  it("今日のコピーは HH:mm、別の日は M/D で表示する", async () => {
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 8, 5).getTime();
    const old = new Date(2020, 0, 31, 8, 5).getTime();
    setup({ items: [clip(1, { copied_at_ms: today }), clip(2, { copied_at_ms: old })] });
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    expect(within(options()[0]).getByText("08:05")).toBeTruthy();
    expect(within(options()[1]).getByText("1/31")).toBeTruthy();
  });
});

describe("HistoryWindow 操作", () => {
  it("Enter で選択行を paste_clip(auto) で貼り付ける", async () => {
    const env = setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 1, mode: "auto" });
  });

  it("↓ キーで選択を移して Enter で貼り付ける(マウスなし)", async () => {
    const env = setup({ items: [clip(1), clip(2), clip(3)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(options()[2].getAttribute("aria-selected")).toBe("true");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 3, mode: "auto" });
  });

  it("マウスが行の上を動いても、選択している行は変わらない", async () => {
    setup({ items: [clip(1), clip(2), clip(3)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
    await user.hover(options()[2]);
    await user.pointer({ target: options()[1] });
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
    expect(options()[1].getAttribute("aria-selected")).toBe("false");
    expect(options()[2].getAttribute("aria-selected")).toBe("false");
  });

  it("行のクリックで貼り付ける", async () => {
    const env = setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.click(options()[1]);
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 2, mode: "auto" });
  });

  it("macOS で Cmd+数字のとき、その番号の行を貼り付ける", async () => {
    const env = setup({ items: [clip(1), clip(2), clip(3)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Meta>}3{/Meta}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 3, mode: "auto" });
  });

  it("Windows で Alt+数字のとき、その番号の行を貼り付ける", async () => {
    const env = setup({ items: [clip(1), clip(2)], status: { ...baseStatus, os: "windows" } });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Alt>}2{/Alt}");
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 2, mode: "auto" });
  });

  it("修飾キーなしの数字は貼り付けずに検索欄へ入力される", async () => {
    const env = setup({ items: [clip(1, { first_line: "a1" }), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.keyboard("1");
    expect((screen.getByRole("searchbox") as HTMLInputElement).value).toBe("1");
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("貼り付け中の Enter 連打は1回だけ paste_clip を呼ぶ", async () => {
    let release: (v: PasteOutcome) => void = () => {};
    const env = setup({ items: [clip(1)] });
    mockIPC(
      (cmd, args) => {
        env.calls.push({ cmd, args });
        if (cmd === "get_app_status") return env.status;
        if (cmd === "list_clips")
          return { req_id: (args as { reqId: number }).reqId, total: 1, items: env.items };
        if (cmd === "paste_clip") return new Promise((r) => (release = r));
        return null;
      },
      { shouldMockEvents: true },
    );
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}{Enter}{Enter}");
    expect(calls(env, "paste_clip").length).toBe(1);
    release({ result: "pasted", reason: null });
  });

  it("Esc で hide_window を呼ぶ", async () => {
    const env = setup({ items: [clip(1)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await user.keyboard("{Escape}");
    expect(calls(env, "hide_window").length).toBe(1);
  });

  it("Cmd+, で設定ウィンドウを開く", async () => {
    const env = setup({ items: [clip(1)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Meta>},{/Meta}");
    expect(calls(env, "open_settings").length).toBe(1);
  });

  it("フッターの「設定」で設定ウィンドウを開く", async () => {
    const env = setup({ items: [clip(1)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await user.click(screen.getByRole("button", { name: "設定" }));
    expect(calls(env, "open_settings").length).toBe(1);
  });
});

describe("HistoryWindow プレビュー", () => {
  it("初期は閉じていて、Tab で開閉し、選択に追従する", async () => {
    const env = setup({
      items: [clip(1), clip(2)],
      details: {
        1: {
          id: 1,
          kind: "text",
          text: "全文1\n2行目",
          image_png: null,
          source_app_name: "Editor",
          copied_at_ms: 0,
          char_count: 7,
        },
        2: {
          id: 2,
          kind: "code",
          text: "const x = 1",
          image_png: null,
          source_app_name: null,
          copied_at_ms: 0,
          char_count: 11,
        },
      },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    expect(screen.queryByRole("region", { name: "プレビュー" })).toBeNull();
    await user.keyboard("{Tab}");
    const pane = await screen.findByRole("region", { name: "プレビュー" });
    await waitFor(() => expect(pane.textContent).toContain("全文1"));
    expect(pane.textContent).toContain("テキスト");
    expect(pane.textContent).toContain("Editor");
    expect(pane.textContent).toContain("7 文字");
    await user.keyboard("{ArrowDown}");
    await waitFor(() => expect(pane.textContent).toContain("const x = 1"));
    expect(pane.textContent).toContain("コード");
    expect(calls(env, "get_clip_detail").length).toBeGreaterThan(1);
    await user.keyboard("{Tab}");
    expect(screen.queryByRole("region", { name: "プレビュー" })).toBeNull();
  });

  it("画像は data URL の img で拡大表示する", async () => {
    setup({
      items: [clip(1, { kind: "image", first_line: "", thumb_png: "AAAA" })],
      details: {
        1: {
          id: 1,
          kind: "image",
          text: null,
          image_png: "BBBB",
          source_app_name: null,
          copied_at_ms: 0,
          char_count: 0,
        },
      },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await user.keyboard("{Tab}");
    const img = await screen.findByAltText("画像のプレビュー");
    expect(img.getAttribute("src")).toBe("data:image/png;base64,BBBB");
  });
});

describe("HistoryWindow 削除とピン留め", () => {
  const focusedId = () => (document.activeElement as HTMLElement | null)?.id;

  it("中間の行を削除すると、フォーカスが次の行に移る", async () => {
    const env = setup({ items: [clip(1), clip(2), clip(3)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(3));
    await user.keyboard("{ArrowDown}");
    expect(focusedId()).toBe("clip-2");
    await user.keyboard("{Delete}");
    await waitFor(() => expect(options().length).toBe(2));
    expect(calls(env, "delete_clip")[0].args).toEqual({ id: 2 });
    await waitFor(() => expect(focusedId()).toBe("clip-3"));
  });

  it("最後の行を削除すると、フォーカスが前の行に移る", async () => {
    setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.keyboard("{ArrowDown}{Delete}");
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(focusedId()).toBe("clip-1"));
  });

  it("最後の1件を削除すると、フォーカスが検索欄に移る", async () => {
    setup({ items: [clip(1)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await user.keyboard("{ArrowDown}{Delete}");
    await waitFor(() => expect(options().length).toBe(0));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("searchbox")));
  });

  it("行にフォーカスがあるときだけ、ピン留めと削除のキー案内を表示する", async () => {
    setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    expect(screen.queryByText("ピン留め", { selector: "span" })).toBeNull();
    expect(screen.queryByText("削除")).toBeNull();
    await user.keyboard("{ArrowDown}");
    expect(await screen.findByText("削除")).toBeTruthy();
    expect(screen.getByText("ピン留め", { selector: "span:not([role])" })).toBeTruthy();
  });

  it("P キーでピン留めを切り替え、ピン留めした行が先頭に移る", async () => {
    const env = setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    await user.keyboard("{ArrowDown}p");
    await waitFor(() => expect(calls(env, "set_pinned").length).toBe(1));
    expect(calls(env, "set_pinned")[0].args).toEqual({ id: 2, pinned: true });
  });
});

describe("HistoryWindow 権限の案内(S4)", () => {
  const noPerm: AppStatus = { ...baseStatus, paste_permission: false };

  it("権限なしで案内が未表示のとき、Enter で貼り付けずに案内を出す", async () => {
    const env = setup({ items: [clip(1)], status: noPerm });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("アクセシビリティの権限");
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("「システム設定を開く」で open_accessibility_settings を呼ぶ", async () => {
    const env = setup({ items: [clip(1)], status: noPerm });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("button", { name: "システム設定を開く" }));
    expect(calls(env, "open_accessibility_settings").length).toBe(1);
  });

  it("「あとで」で非表示を保存し、copy_only で貼り付ける", async () => {
    const env = setup({
      items: [clip(1)],
      status: noPerm,
      outcome: { result: "copied_only", reason: "no_permission" },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("button", { name: "あとで(再コピーのみ)" }));
    await waitFor(() => expect(calls(env, "paste_clip").length).toBe(1));
    expect(calls(env, "update_settings")[0].args).toEqual({
      patch: { permission_guide_dismissed: true },
    });
    expect(calls(env, "paste_clip")[0].args).toEqual({ id: 1, mode: "copy_only" });
    await screen.findByText("自動貼り付けには権限が必要です(再コピーのみ)");
  });

  it("案内の表示中に Esc を押すと、案内だけを閉じてウィンドウは閉じない", async () => {
    const env = setup({ items: [clip(1)], status: noPerm });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls(env, "hide_window").length).toBe(0);
    expect(calls(env, "paste_clip").length).toBe(0);
  });

  it("案内を非表示にした後は、権限なしのときフッターに案内を表示する", async () => {
    setup({ items: [clip(1)], status: { ...noPerm, permission_guide_dismissed: true } });
    render(<HistoryWindow />);
    await screen.findByText("自動貼り付けには権限が必要です(再コピーのみ)");
  });

  it("結果の reason が send_failed のとき、送信失敗の案内を表示する", async () => {
    const env = setup({
      items: [clip(1)],
      outcome: { result: "copied_only", reason: "send_failed" },
    });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(1));
    await waitFor(() => expect(calls(env, "get_app_status").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    await screen.findByText("貼り付けの送信に失敗しました(内容はコピー済みです)");
  });
});

describe("HistoryWindow 鍵が使えない状態(H-4)", () => {
  it("storage が key_unavailable のとき、一覧の代わりに理由を表示する", async () => {
    setup({
      items: [clip(1)],
      status: { ...baseStatus, storage: "key_unavailable", storage_message: "keychain locked" },
    });
    render(<HistoryWindow />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain(
      "暗号鍵を取得できないため、履歴を読み込んでいません。保存済みの履歴は変更していません",
    );
    expect(options().length).toBe(0);
    expect(screen.queryByRole("searchbox")).toBeNull();
  });

  it("storage が key_mismatch のとき、別の理由を表示する", async () => {
    setup({ status: { ...baseStatus, storage: "key_mismatch" } });
    render(<HistoryWindow />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("一致しない");
  });
});

describe("HistoryWindow ウィンドウの高さ", () => {
  it("内容に合わせて set_size を呼び、プレビューを開くと 96px 高くなる", async () => {
    const env = setup({ items: [clip(1), clip(2)] });
    const user = userEvent.setup();
    render(<HistoryWindow />);
    await waitFor(() => expect(options().length).toBe(2));
    type Sz = { Logical: { width: number; height: number } };
    // Size インスタンスは JSON にすると { Logical: { width, height } } になる
    const sizes = () =>
      env.calls
        .filter((c) => c.cmd === "plugin:window|set_size")
        .map((c) => JSON.parse(JSON.stringify((c.args as { value: unknown }).value)) as Sz);
    await waitFor(() => expect(sizes().length).toBeGreaterThan(0));
    const last = () => sizes()[sizes().length - 1];
    const h = (v: Sz) => v.Logical.height;
    const before = h(last());
    expect(last().Logical.width).toBe(420);
    expect(before).toBeLessThanOrEqual(330);
    await user.keyboard("{Tab}");
    await waitFor(() => expect(h(last())).toBe(before + 96));
  });
});
