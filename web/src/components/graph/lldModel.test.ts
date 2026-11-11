import { describe, expect, it } from "vitest";
import { lldParts, lldTitle } from "./lldModel";

describe("LLD titles", () => {
  it("read without their level marks", () => {
    expect(lldTitle("Mobile flow: vehicle list to live map (LLD)")).toBe("Vehicle list to live map");
    expect(lldTitle("API request with tenancy and permissions (LLD flow)")).toBe("API request with tenancy and permissions");
    expect(lldTitle("ArgoCD structure, projects, roots and sync waves (infra LLD)")).toBe("ArgoCD structure, projects, roots and sync waves");
    expect(lldTitle("Application (LLD component)")).toBe("Application");
    expect(lldTitle("Orders")).toBe("Orders");
  });
});

describe("LLD parts", () => {
  it("sorts docs by what they describe; a doc that does not say goes under reference", () => {
    const parts = lldParts([
      { id: "b", title: "Zones (LLD flow)", kind: "flow" },
      { id: "a", title: "Alerts (LLD flow)", kind: "flow" },
      { id: "c", title: "Auth", kind: "component" },
      { id: "d", title: "Glossary" },
    ]);
    expect(parts.flows.map((doc) => doc.id)).toEqual(["a", "b"]);
    expect(parts.components.map((doc) => doc.id)).toEqual(["c"]);
    expect(parts.reference.map((doc) => doc.id)).toEqual(["d"]);
  });
});
