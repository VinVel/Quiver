import type { ButtonHTMLAttributes, ReactNode } from "react";
import { classNames } from "./classNames";

type ButtonVariant = "primary" | "secondary" | "ghost" | "icon" | "destructive";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  fullWidth?: boolean;
  iconOnly?: boolean;
  children: ReactNode;
};

export function Button({
  variant = "secondary",
  fullWidth = false,
  iconOnly = false,
  className,
  children,
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      className={classNames(
        "ui-button",
        `ui-button--${variant}`,
        fullWidth && "ui-button--full-width",
        iconOnly && "ui-button--icon-only",
        className,
      )}
      type={type}
      {...props}
    >
      {children}
    </button>
  );
}
