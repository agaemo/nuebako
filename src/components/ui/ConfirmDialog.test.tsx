import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConfirmDialog } from "./ConfirmDialog";

function setup(props: { busy?: boolean } = {}) {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  render(
    <ConfirmDialog
      title="全履歴を削除"
      message="3 件の履歴をすべて削除します。元に戻せません"
      confirmLabel="削除"
      onConfirm={onConfirm}
      onCancel={onCancel}
      {...props}
    />,
  );
  return { onConfirm, onCancel };
}

describe("ConfirmDialog", () => {
  it("開いたとき、初期フォーカスはキャンセルにある", () => {
    setup();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "キャンセル" }));
  });

  it("タイトルと説明が、ダイアログの名前と説明になる", () => {
    setup();
    const dialog = screen.getByRole("dialog", { name: "全履歴を削除" });
    expect(dialog.getAttribute("aria-describedby")).toBe("confirm-message");
  });

  it("削除を押すとonConfirm、キャンセルを押すとonCancelが呼ばれる", async () => {
    const { onConfirm, onCancel } = setup();
    await userEvent.click(screen.getByRole("button", { name: "削除" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("Escとバックドロップのクリックで閉じる(onCancel)", async () => {
    const { onCancel } = setup();
    await userEvent.keyboard("{Escape}");
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.mouseDown(screen.getByTestId("dialog-backdrop"));
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it("処理中は、Escとバックドロップのクリックで閉じず、両方のボタンも無視される", async () => {
    const { onConfirm, onCancel } = setup({ busy: true });
    await userEvent.keyboard("{Escape}");
    fireEvent.mouseDown(screen.getByTestId("dialog-backdrop"));
    await userEvent.click(screen.getByRole("button", { name: "削除" }));
    await userEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    expect(onCancel).not.toHaveBeenCalled();
    expect(onConfirm).not.toHaveBeenCalled();
    // フォーカスは外れない
    expect(document.activeElement?.tagName).toBe("BUTTON");
  });

  it("Tabはダイアログの2つのボタンの間だけを循環する", async () => {
    setup();
    const cancel = screen.getByRole("button", { name: "キャンセル" });
    const confirm = screen.getByRole("button", { name: "削除" });
    await userEvent.tab();
    expect(document.activeElement).toBe(confirm);
    await userEvent.tab();
    expect(document.activeElement).toBe(cancel);
    await userEvent.tab({ shift: true });
    expect(document.activeElement).toBe(confirm);
  });
});
