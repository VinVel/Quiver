import { Eye, EyeOff } from "lucide-react";
import type { InputHTMLAttributes, ReactNode } from "react";
import { useId, useState } from "react";
import { classNames } from "./classNames";

type TextFieldProps = InputHTMLAttributes<HTMLInputElement> & {
  label: string;
  isInvalid?: boolean;
  isRequiredVisible?: boolean;
  helperText?: ReactNode;
  controlClassName?: string;
};

export function TextField({
  label,
  isInvalid = false,
  isRequiredVisible = false,
  helperText,
  className,
  controlClassName,
  id,
  type,
  disabled,
  readOnly,
  ...inputProps
}: TextFieldProps) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const isPasswordField = type === "password";
  const [isPasswordVisible, setIsPasswordVisible] = useState(false);
  const resolvedType = isPasswordField && isPasswordVisible ? "text" : type;

  return (
    <div className={classNames("ui-field", className)}>
      <label className="ui-field__label" htmlFor={inputId}>
        {label}
        {isRequiredVisible ? (
          <span className="ui-required-marker" aria-hidden="true">
            *
          </span>
        ) : null}
      </label>
      <span className={classNames(isPasswordField && "ui-field__control-wrap")}>
        <input
          className={classNames(
            "ui-field__control",
            isPasswordField && "ui-field__control--with-trailing-action",
            controlClassName,
          )}
          data-invalid={isInvalid || undefined}
          disabled={disabled}
          id={inputId}
          readOnly={readOnly}
          type={resolvedType}
          {...inputProps}
        />
        {isPasswordField ? (
          <button
            aria-label={isPasswordVisible ? `Hide ${label}` : `Show ${label}`}
            className="ui-field__trailing-action"
            disabled={disabled || readOnly}
            onClick={() => setIsPasswordVisible((current) => !current)}
            type="button"
          >
            {isPasswordVisible ? (
              <EyeOff aria-hidden="true" />
            ) : (
              <Eye aria-hidden="true" />
            )}
          </button>
        ) : null}
      </span>
      {helperText ? (
        <span className="ui-field__helper">{helperText}</span>
      ) : null}
    </div>
  );
}
