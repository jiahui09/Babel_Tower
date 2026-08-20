import { forwardRef } from "react";
import type { TextareaHTMLAttributes } from "react";

import { cn } from "../../lib/utils";

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(
  ({ className, ...props }, ref) => (
    <textarea
      ref={ref}
      className={cn(
        "min-h-[84px] w-full resize-y border border-[var(--border)] bg-[var(--surface)] p-3 text-sm leading-6 outline-none focus:border-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-60",
        className,
      )}
      {...props}
    />
  ),
);

Textarea.displayName = "Textarea";
