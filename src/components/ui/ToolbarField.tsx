import type { InputHTMLAttributes, ReactNode } from "react";
import { classNames } from "./classNames";

type ToolbarFieldProps = InputHTMLAttributes<HTMLInputElement> & {
  icon?: ReactNode;
};

export function ToolbarField({
  className,
  icon,
  type = "text",
  ...props
}: ToolbarFieldProps) {
  return (
    <label className={classNames("ui-toolbar-field", className)}>
      {icon ? <span className="ui-toolbar-field__icon">{icon}</span> : null}
      <input className="ui-toolbar-field__input" type={type} {...props} />
    </label>
  );
}
