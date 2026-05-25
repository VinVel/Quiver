import type { HTMLAttributes, ReactNode } from "react";
import { classNames } from "./classNames";

type EmptyStateProps = HTMLAttributes<HTMLElement> & {
  title: ReactNode;
  copy: ReactNode;
  graphic?: ReactNode;
  actions?: ReactNode;
};

export function EmptyState({
  title,
  copy,
  graphic,
  actions,
  className,
  ...props
}: EmptyStateProps) {
  return (
    <section className={classNames("ui-empty-state", className)} {...props}>
      {graphic ? (
        <div className="ui-empty-state__graphic">{graphic}</div>
      ) : null}
      <div className="ui-empty-state__copy">
        <h2 className="ui-empty-state__title">{title}</h2>
        <p className="ui-empty-state__text">{copy}</p>
      </div>
      {actions ? (
        <div className="ui-empty-state__actions">{actions}</div>
      ) : null}
    </section>
  );
}
