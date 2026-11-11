import * as Primitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

const Dialog = Primitive.Root;
const DialogTitle = Primitive.Title;

function Overlay() {
  return <Primitive.Overlay className="fixed inset-0 z-50 bg-black/40 data-[state=open]:animate-in data-[state=open]:fade-in-0" />;
}

/** A box near the top of the screen. */
function DialogContent({ className, ...props }: ComponentProps<typeof Primitive.Content>) {
  return (
    <Primitive.Portal>
      <Overlay />
      <Primitive.Content
        aria-describedby={undefined}
        className={cn(
          "fixed top-[12vh] left-1/2 z-50 w-[calc(100vw-2rem)] max-w-2xl -translate-x-1/2 rounded-lg border bg-popover text-popover-foreground shadow-xl outline-none",
          className,
        )}
        {...props}
      />
    </Primitive.Portal>
  );
}

/** A panel along the left edge, for the sidebar on a phone. */
function SheetContent({ className, children, ...props }: ComponentProps<typeof Primitive.Content>) {
  return (
    <Primitive.Portal>
      <Overlay />
      <Primitive.Content
        aria-describedby={undefined}
        className={cn(
          "fixed inset-y-0 left-0 z-50 flex w-[min(18rem,85vw)] flex-col border-r bg-sidebar outline-none data-[state=open]:animate-in data-[state=open]:slide-in-from-left",
          className,
        )}
        {...props}
      >
        {children}
        <Primitive.Close
          aria-label="Close menu"
          className="absolute top-3 right-3 flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          <X className="size-4" />
        </Primitive.Close>
      </Primitive.Content>
    </Primitive.Portal>
  );
}

export { Dialog, DialogContent, DialogTitle, SheetContent };
