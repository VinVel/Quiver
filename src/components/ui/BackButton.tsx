import { ChevronLeft } from "lucide-react";
import type { ButtonHTMLAttributes } from "react";
import { Button } from "./Button";
import { classNames } from "./classNames";

type BackButtonProps = Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "children"
> & {
  overlay?: boolean;
};

export function BackButton({
  "aria-label": ariaLabel = "Back",
  className,
  overlay = false,
  ...props
}: BackButtonProps) {
  return (
    <Button
      aria-label={ariaLabel}
      className={classNames(
        "ui-back-button",
        overlay && "ui-back-button--overlay",
        className,
      )}
      iconOnly
      variant="ghost"
      {...props}
    >
      <ChevronLeft aria-hidden="true" />
    </Button>
  );
}
