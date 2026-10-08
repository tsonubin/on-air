import type { ComponentPropsWithRef } from "react";

export type ButtonVariant =
  | "action"
  | "key"
  | "key-main"
  | "primary"
  | "secondary"
  | "ghost"
  | "step";

const VARIANT_CLASS: Record<ButtonVariant, string> = {
  action: "btn",
  key: "btn btn-key",
  "key-main": "btn btn-key btn-key-main",
  primary: "btn btn-primary",
  secondary: "btn btn-secondary",
  ghost: "btn btn-ghost",
  step: "btn btn-step",
};

export interface ButtonProps extends ComponentPropsWithRef<"button"> {
  variant?: ButtonVariant;
}

/** The one button. Styles live in `App.css` under `.btn`. */
export function Button({ variant = "action", className, type = "button", ...rest }: ButtonProps) {
  const classes = className ? `${VARIANT_CLASS[variant]} ${className}` : VARIANT_CLASS[variant];
  return <button type={type} className={classes} {...rest} />;
}
