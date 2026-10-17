interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** スクリーンリーダー向けの名前 */
  label: string;
}

/** オン・オフの切り替え。 */
export function Toggle({ checked, onChange, label }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-4 w-7 rounded-full border border-line ${checked ? "bg-accent" : "bg-surface"}`}
    >
      <span
        className={`absolute top-px h-3 w-3 rounded-full ${checked ? "left-[13px] bg-on-accent" : "left-px bg-text-sub"}`}
      />
    </button>
  );
}
