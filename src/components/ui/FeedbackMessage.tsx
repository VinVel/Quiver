import type { HTMLAttributes, ReactNode } from "react";
import { classNames } from "./classNames";

export type FeedbackTone = "error" | "success" | "info" | "warning";

type FeedbackMessageProps = HTMLAttributes<HTMLParagraphElement> & {
  tone: FeedbackTone;
  children: ReactNode;
};

export function FeedbackMessage({
  tone,
  className,
  children,
  ...props
}: FeedbackMessageProps) {
  return (
    <p
      className={classNames("ui-feedback", `ui-feedback--${tone}`, className)}
      {...props}
    >
      {children}
    </p>
  );
}
