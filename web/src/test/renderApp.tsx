import { render } from "@testing-library/react";
import { App } from "@/App";
import { createMemoryRouter, createQueryClient } from "@/router";

/** Renders the whole app at `path` and returns the router, so a test can read where it ended up. */
export function renderApp(path: string) {
  const router = createMemoryRouter(createQueryClient(), path);
  render(<App router={router} />);
  return router;
}
