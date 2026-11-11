import { createFileRoute, redirect } from "@tanstack/react-router";

// There is no list of every item; the docs are the front door.
export const Route = createFileRoute("/items/")({
  beforeLoad: () => {
    throw redirect({ to: "/docs", replace: true });
  },
});
