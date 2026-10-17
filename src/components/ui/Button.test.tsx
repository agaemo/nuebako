import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button } from "./Button";

describe("Button", () => {
  it("押すとonClickが呼ばれる", async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>実行</Button>);
    await userEvent.click(screen.getByRole("button", { name: "実行" }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("busyのとき、押してもonClickは呼ばれず、フォーカスは保たれる", async () => {
    const onClick = vi.fn();
    render(
      <Button busy onClick={onClick}>
        実行
      </Button>,
    );
    const button = screen.getByRole("button", { name: "実行" });
    button.focus();
    await userEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(document.activeElement).toBe(button);
  });
});
