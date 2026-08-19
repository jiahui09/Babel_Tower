import * as SwitchPrimitive from "@radix-ui/react-switch";
import * as React from "react";

import { cn } from "../../lib/utils";

export const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitive.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitive.Root
    ref={ref}
    className={cn(
      "h-5 w-9 rounded-full border border-[var(--border-strong)] bg-[var(--surface-inset)] transition-colors data-[state=checked]:border-[var(--accent)] data-[state=checked]:bg-[var(--accent)]",
      className,
    )}
    {...props}
  >
    <SwitchPrimitive.Thumb className="block size-4 translate-x-0.5 rounded-full bg-white shadow-sm transition-transform data-[state=checked]:translate-x-[17px]" />
  </SwitchPrimitive.Root>
));
Switch.displayName = "Switch";

