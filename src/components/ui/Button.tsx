import type { ComponentProps } from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

const variants: Record<ButtonVariant, string> = {
  primary: "bg-accent text-on-accent border-accent",
  secondary: "bg-surface text-text border-line",
  ghost: "bg-transparent text-text border-transparent",
  danger: "bg-transparent text-accent border-accent",
};

interface ButtonProps extends Omit<ComponentProps<"button">, "disabled"> {
  variant?: ButtonVariant;
  /** 処理中。disabled にするとフォーカスが外れるため、aria-disabled にして押下を無視する */
  busy?: boolean;
}

/** ボタン。busy のときは押しても何も起きないが、フォーカスは保つ。 */
export function Button({ variant = "secondary", busy = false, onClick, className = "", ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      {...rest}
      aria-disabled={busy || undefined}
      onClick={(e) => {
        if (busy) return;
        onClick?.(e);
      }}
      className={`rounded-md border px-3 py-0.5 text-[12px] aria-disabled:opacity-60 ${variants[variant]} ${className}`}
    />
  );
}
