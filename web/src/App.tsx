import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import type { createAppRouter } from "./router";
import { ThemeProvider } from "./shell/theme";

export function App({ router }: { router: ReturnType<typeof createAppRouter> }) {
  return (
    <QueryClientProvider client={router.options.context.queryClient}>
      <ThemeProvider>
        <RouterProvider router={router} />
      </ThemeProvider>
    </QueryClientProvider>
  );
}
