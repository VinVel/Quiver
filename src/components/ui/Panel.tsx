import type { HTMLAttributes, ReactNode } from "react";
import { classNames } from "./classNames";

type PanelProps = HTMLAttributes<HTMLElement> & {
  children: ReactNode;
  narrow?: boolean;
  as?: "section" | "article" | "div";
};

export function Panel({
  as: Element = "section",
  narrow = false,
  className,
  children,
  ...props
}: PanelProps) {
  return (
    <Element
      className={classNames(
        "ui-panel",
        narrow && "ui-panel--narrow",
        className,
      )}
      {...props}
    >
      {children}
    </Element>
  );
}

export function Card({
  className,
  children,
  ...props
}: HTMLAttributes<HTMLElement>) {
  return (
    <article className={classNames("ui-card", className)} {...props}>
      {children}
    </article>
  );
}
