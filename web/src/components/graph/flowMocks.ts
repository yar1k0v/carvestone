import { vi } from "vitest";

/**
 * jsdom has no layout engine; React Flow reads sizes through these. Call before rendering a flow and
 * undo with `vi.unstubAllGlobals()`.
 */
export function installFlowMocks() {
  class FakeResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  class FakeMatrix {
    m22: number;
    constructor(transform?: string) {
      const scale = /scale\(([\d.]+)\)/.exec(transform ?? "");
      this.m22 = scale?.[1] === undefined ? 1 : Number(scale[1]);
    }
  }
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  vi.stubGlobal("DOMMatrixReadOnly", FakeMatrix);
}
