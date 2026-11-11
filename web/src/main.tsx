import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { createAppRouter, createQueryClient } from "./router";

const root = document.getElementById("root");
if (root === null) throw new Error("index.html has no #root element");

createRoot(root).render(
  <StrictMode>
    <App router={createAppRouter(createQueryClient())} />
  </StrictMode>,
);
