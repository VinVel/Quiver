import type { HTMLAttributes, ReactNode } from "react";
import { classNames } from "./classNames";

type PillTone = "neutral" | "primary" | "secondary";

type PillProps = HTMLAttributes<HTMLSpanElement> & {
  tone?: PillTone;
  children: ReactNode;
};

export function Pill({
  tone = "neutral",
  className,
  children,
  ...props
}: PillProps) {
  return (
    <span
      className={classNames("ui-pill", `ui-pill--${tone}`, className)}
      {...props}
    >
      {children}
    </span>
  );
}
