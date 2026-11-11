import { createFileRoute, redirect } from "@tanstack/react-router";

// Old links keep working; their filters carry over.
export const Route = createFileRoute("/timeline")({
  beforeLoad: ({ search }) => {
    throw redirect({ to: "/activity", search, replace: true });
  },
});
