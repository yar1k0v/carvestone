import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";

// The first render in a file compiles the lazy route chunks, which takes longer than the 1 s default.
configure({ asyncUtilTimeout: 5000 });

// jsdom does not implement scrolling; the router calls it on navigation.
window.scrollTo = () => undefined;

afterEach(cleanup);
