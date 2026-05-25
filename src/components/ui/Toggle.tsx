import type { ButtonHTMLAttributes } from "react";
import { classNames } from "./classNames";

type ToggleProps = Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "aria-pressed"
> & {
  checked: boolean;
  label: string;
};

export function Toggle({
  checked,
  label,
  className,
  type = "button",
  ...props
}: ToggleProps) {
  return (
    <button
      aria-label={label}
      aria-pressed={checked}
      className={classNames(
        "ui-toggle",
        checked && "ui-toggle--checked",
        className,
      )}
      type={type}
      {...props}
    >
      <span className="ui-toggle__track" aria-hidden="true">
        <span className="ui-toggle__thumb" />
      </span>
    </button>
  );
}
