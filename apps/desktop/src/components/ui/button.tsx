import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "../../lib/utils";

export const buttonVariants = cva(
  "inline-flex h-8 shrink-0 items-center justify-center gap-2 rounded-[6px] border text-sm font-medium transition-colors duration-150 disabled:pointer-events-none disabled:opacity-45",
  {
    variants: {
      variant: {
        primary: "border-[var(--accent)] bg-[var(--accent)] px-3 text-white hover:bg-[var(--accent-hover)]",
        secondary:
          "border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[var(--text)] hover:bg-[var(--surface-inset)]",
        ghost:
          "border-transparent bg-transparent px-2 text-[var(--text-secondary)] hover:bg-[var(--surface-inset)] hover:text-[var(--text)]",
        danger: "border-[var(--danger)] bg-[var(--danger)] px-3 text-white",
        icon: "size-8 border-transparent bg-transparent p-0 text-[var(--text-secondary)] hover:bg-[var(--surface-inset)] hover:text-[var(--text)]",
      },
    },
    defaultVariants: {
      variant: "secondary",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, type = "button", ...props }, ref) => (
    <button ref={ref} type={type} className={cn(buttonVariants({ variant }), className)} {...props} />
  ),
);
Button.displayName = "Button";
