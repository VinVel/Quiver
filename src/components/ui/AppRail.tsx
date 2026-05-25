import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from "react";
import { classNames } from "./classNames";

type AppRailProps = HTMLAttributes<HTMLElement> & {
  children: ReactNode;
};

type AppRailButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  children: ReactNode;
  isActive?: boolean;
};

export function AppRail({ className, children, ...props }: AppRailProps) {
  return (
    <aside className={classNames("ui-app-rail", className)} {...props}>
      {children}
    </aside>
  );
}

export function AppRailButton({
  className,
  children,
  isActive = false,
  type = "button",
  ...props
}: AppRailButtonProps) {
  return (
    <button
      className={classNames(
        "ui-app-rail-button",
        isActive && "ui-app-rail-button--active",
        className,
      )}
      type={type}
      {...props}
    >
      {children}
    </button>
  );
}
