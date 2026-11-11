import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installFlowMocks } from "@/components/graph/flowMocks";
import { buildResolver } from "@/components/markdown/links";
import { Markdown } from "@/components/markdown/Markdown";
import { listRow } from "@/test/fixtures";
import { type Box, drawC4, halfway, LEAF_WIDTH, layoutC4, leafHeight, nameSize, placeC4, relText, typeTag } from "./c4Layout";
import { type C4Diagram, type C4Element, isC4, parseC4 } from "./parse";

const mermaid = vi.hoisted(() => ({ initialize: vi.fn(), render: vi.fn() }));
vi.mock("mermaid", () => ({ default: mermaid }));

beforeEach(() => installFlowMocks());
afterEach(() => vi.unstubAllGlobals());

const SHOP = `C4Container
  title Shop containers
  %% a comment
  Person(buyer, "Buyer", "Orders things")
  System_Ext(pay, "Payments", "Takes the money")
  System_Boundary(shop, "Shop") {
    Container(api, "API", "Rust, axum", "Serves every client", $link="arch/api")
    ContainerDb(db, "Orders DB", "PostgreSQL", "Keeps orders")
    ContainerQueue_Ext(bus, "Bus", "Kafka")
  }
  Rel(buyer, api, "Places orders", "HTTPS")
  Rel(api, db, "Reads and writes", $techn="SQL", $descr="Every order")
  Rel_Back(pay, api, "Charges")
  BiRel(api, bus, "Events")
  Rel(api, ghost, "Never drawn")
  UpdateRelStyle(buyer, api, $offsetY="-40")
  Frobnicate(x)
`;

function diagram(source: string): C4Diagram {
  const parsed = parseC4(source);
  if ("error" in parsed) throw new Error(parsed.error);
  return parsed.diagram;
}

describe("C4 parsing", () => {
  it("reads elements, nesting, shapes and named arguments", () => {
    const d = diagram(SHOP);
    expect(d.level).toBe("C4Container");
    expect(d.title).toBe("Shop containers");
    expect(d.elements.map((e) => [e.alias, e.kind, e.shape, e.external, e.parent ?? null])).toEqual([
      ["buyer", "person", "box", false, null],
      ["pay", "system", "box", true, null],
      ["shop", "boundary", "box", false, null],
      ["api", "container", "box", false, "shop"],
      ["db", "container", "db", false, "shop"],
      ["bus", "container", "queue", true, "shop"],
    ]);
    expect(d.elements[3]).toMatchObject({ technology: "Rust, axum", description: "Serves every client", link: "arch/api" });
    expect(d.elements[0]).toMatchObject({ description: "Orders things" });
    expect(d.elements[2]).toMatchObject({ label: "Shop", type: "system" });
  });

  it("reads relationships, turns Rel_Back around and keeps the ones it cannot draw as skipped", () => {
    const d = diagram(SHOP);
    expect(d.rels.map((r) => [r.from, r.to, relText(r), r.bidirectional])).toEqual([
      ["buyer", "api", "Places orders [HTTPS]", false],
      ["api", "db", "Reads and writes [SQL]", false],
      ["api", "pay", "Charges", false],
      ["api", "bus", "Events", true],
    ]);
    expect(d.rels[1]?.description).toBe("Every order");
    expect(d.skipped).toEqual([
      { line: 15, text: 'Rel(api, ghost, "Never drawn")' },
      { line: 17, text: "Frobnicate(x)" },
    ]);
  });

  it("numbers the steps of a dynamic diagram", () => {
    const d = diagram('C4Dynamic\n  Container(a, "A")\n  Container(b, "B")\n  Rel(a, b, "Asks")\n  RelIndex(7, b, a, "Answers")\n');
    expect(d.rels.map(relText)).toEqual(["1. Asks", "7. Answers"]);
  });

  it("knows a C4 diagram by its first line", () => {
    expect(isC4("\n  C4Context\n")).toBe(true);
    expect(isC4("graph TD\n  C4Context")).toBe(false);
    expect(parseC4("flowchart LR")).toEqual({ error: expect.stringContaining("starts with C4Context") });
  });
});

// The texts `stone arch c4` writes for the shop fixture of crates/engine/tests/features/c4.rs.
const GENERATED_CONTEXT = `C4Context
  title System context diagram for Shop
  Person(arch_buyer, "Buyer", "Buys things.", $link="arch/buyer")
  System_Ext(arch_pay, "Payments", "Takes the money.", $link="arch/pay")
  System(arch_shop, "Shop", "Sells things online.", $link="arch/shop")
  Rel(arch_buyer, arch_shop, "Browses items", "HTTPS")
  Rel(arch_shop, arch_pay, "Charges cards, Sends card details", "HTTPS, HTTP")
`;

const GENERATED_CONTAINER = `C4Container
  title Container diagram for Shop
  Person(arch_buyer, "Buyer", "Buys things.", $link="arch/buyer")
  System_Ext(arch_pay, "Payments", "Takes the money.", $link="arch/pay")
  System_Boundary(arch_shop, "Shop") {
    Container(arch_shop_back_api, "Api", "Rust", "Serves orders.", $link="arch/shop/back/api")
    ContainerDb(arch_shop_back_db, "Orders DB", "PostgreSQL", "Keeps every order.", $link="arch/shop/back/db")
    Container(arch_shop_web_site, "Site", "React", "Shows the shop.", $link="arch/shop/web/site")
  }
  Rel(arch_buyer, arch_shop_web_site, "Browses items", "HTTPS")
  Rel(arch_shop_back_api, arch_pay, "Charges cards", "HTTPS")
  Rel(arch_shop_back_api, arch_shop_back_db, "Saves orders", "SQL")
  Rel(arch_shop_web_site, arch_pay, "Sends card details", "HTTP")
  Rel(arch_shop_web_site, arch_shop_back_api, "Places orders", "HTTP")
`;

const GENERATED_COMPONENT = `C4Component
  title Component diagram for Api
  System_Ext(arch_pay, "Payments", "Takes the money.", $link="arch/pay")
  ContainerDb(arch_shop_back_db, "Orders DB", "PostgreSQL", "Keeps every order.", $link="arch/shop/back/db")
  System(arch_shop_web, "Web shop", "Takes the orders.", $link="arch/shop/web")
  Container_Boundary(arch_shop_back_api, "Api") {
    Component(arch_shop_back_api_orders, "Orders", $link="arch/shop/back/api/orders")
    Component(arch_shop_back_api_routes, "Routes", "", "Maps URLs.", $link="arch/shop/back/api/routes")
  }
  Rel(arch_shop_back_api, arch_pay, "Charges cards", "HTTPS")
  Rel(arch_shop_back_api, arch_shop_back_db, "Saves orders", "SQL")
  Rel(arch_shop_back_api_orders, arch_shop_back_db, "Writes rows", "SQL")
  Rel(arch_shop_back_api_routes, arch_shop_back_api_orders, "2 references")
  Rel(arch_shop_web, arch_shop_back_api, "Places orders", "HTTP")
`;

describe("C4 diagrams stone generates", () => {
  it("reads every element and line of the three levels, and none is left over", () => {
    // [text, elements, lines]
    const levels: [string, number, number][] = [
      [GENERATED_CONTEXT, 3, 2],
      [GENERATED_CONTAINER, 6, 5],
      [GENERATED_COMPONENT, 6, 5],
    ];
    for (const [source, elements, rels] of levels) {
      const d = diagram(source);
      expect([d.elements.length, d.rels.length, d.skipped.length]).toEqual([elements, rels, 0]);
      expect(d.elements.every((e) => e.link !== undefined || e.kind === "boundary")).toBe(true);
    }
    const container = diagram(GENERATED_CONTAINER);
    expect(container.elements.find((e) => e.alias === "arch_shop_back_db")).toMatchObject({ shape: "db", parent: "arch_shop" });
    const component = diagram(GENERATED_COMPONENT);
    expect(component.elements.find((e) => e.alias === "arch_shop_back_api_routes")).toMatchObject({
      kind: "component",
      description: "Maps URLs.",
      parent: "arch_shop_back_api",
    });
    expect(component.rels.map(relText)).toContain("2 references");
  });

  it("reads #quot; as a double quote in a label and a description", () => {
    const d = diagram('C4Context\n  System(q, "The #quot;Shop#quot;", "Says #quot;hi#quot;, twice.", $link="arch/q")\n');
    expect(d.elements[0]).toMatchObject({ label: 'The "Shop"', description: 'Says "hi", twice.' });
  });
});

describe("C4 layout", () => {
  it("puts children inside their boundary, parent first, and labels the arrows", async () => {
    const { nodes, edges } = await layoutC4(diagram(SHOP));
    const shop = nodes.find((n) => n.id === "shop");
    expect(shop?.type).toBe("c4Group");
    expect(nodes.findIndex((n) => n.id === "shop")).toBeLessThan(nodes.findIndex((n) => n.id === "api"));
    for (const id of ["api", "db", "bus"]) {
      const child = nodes.find((n) => n.id === id);
      expect(child).toMatchObject({ type: "c4", parentId: "shop", width: LEAF_WIDTH });
      expect((child?.position.x ?? -1) + (child?.width ?? 0)).toBeLessThanOrEqual(shop?.width ?? 0);
      expect((child?.position.y ?? -1) + (child?.height ?? 0)).toBeLessThanOrEqual(shop?.height ?? 0);
      expect(child?.position.y).toBeGreaterThanOrEqual(0);
    }
    expect(edges.map((e) => e.label)).toContain("Places orders [HTTPS]");
    expect(edges.find((e) => e.target === "bus")?.markerStart).toBeDefined();
  });

  it("routes every arrow around the boxes it does not connect", async () => {
    // Arrows that skip a layer, cross a boundary and come back up are the ones that used to cut boxes.
    const d = diagram(`C4Container
  Person(u, "User")
  System_Ext(ext, "Outside")
  System_Boundary(s, "System") {
    Container(a, "A", "Rust")
    Container(b, "B", "Rust")
    Container(c, "C", "Rust")
  }
  ContainerDb(db, "DB", "SQL")
  Rel(u, a, "Uses")
  Rel(a, b, "Calls")
  Rel(b, c, "Calls")
  Rel(a, c, "Skips")
  Rel(u, db, "Reads")
  Rel(c, db, "Writes")
  Rel(a, db, "Writes")
  Rel(db, ext, "Copies")
  Rel(ext, u, "Notifies")
`);
    const geometry = await placeC4(d);
    const absolute = (alias: string): Box => {
      const box = geometry.boxes.get(alias) as Box;
      const parent = d.elements.find((e) => e.alias === alias)?.parent;
      const outer = parent === undefined ? { x: 0, y: 0 } : absolute(parent);
      return { ...box, x: box.x + outer.x, y: box.y + outer.y };
    };
    const leaves = d.elements.filter((e) => e.kind !== "boundary").map((e) => [e.alias, absolute(e.alias)] as const);
    expect(geometry.routes.size).toBe(d.rels.length);
    d.rels.forEach((rel, at) => {
      const points = geometry.routes.get(at) ?? [];
      for (const [alias, box] of leaves) {
        if (alias === rel.from || alias === rel.to) continue;
        points.slice(1).forEach((p, i) => {
          const q = points[i] as { x: number; y: number };
          // Orthogonal segments: one crosses a box when it overlaps its inside on both axes.
          const crosses =
            Math.max(p.x, q.x) > box.x + 1 &&
            Math.min(p.x, q.x) < box.x + box.width - 1 &&
            Math.max(p.y, q.y) > box.y + 1 &&
            Math.min(p.y, q.y) < box.y + box.height - 1;
          expect(crosses, `${rel.from}->${rel.to} crosses ${alias}`).toBe(false);
        });
      }
    });
  });

  it("labels can be kept to the selection's arrows", async () => {
    const d = diagram('C4Container\n  Container(a, "A")\n  Container(b, "B")\n  Container(c, "C")\n  Rel(a, b, "Asks")\n  Rel(b, a, "Answers")\n  Rel(b, c, "Logs")\n');
    const { edges } = await layoutC4(d, "a", "selected");
    expect(edges.map((e) => e.label ?? null)).toEqual(["Asks", "Answers", null]);
    expect(halfway([{ x: 0, y: 0 }, { x: 0, y: 10 }, { x: 30, y: 10 }])).toEqual({ x: 10, y: 10 });
  });

  it("writes a box's type in brackets and makes it tall enough for its description", () => {
    const d = diagram(SHOP);
    const tag = (alias: string) => typeTag(d.elements.find((e) => e.alias === alias) as C4Element);
    expect(["buyer", "pay", "api", "db", "bus"].map(tag)).toEqual([
      "[Person]",
      "[Software System, outside]",
      "[Container: Rust, axum]",
      "[Container (database): PostgreSQL]",
      "[Container (queue): Kafka, outside]",
    ]);
    const plain = { alias: "x", kind: "container", shape: "box", external: false, label: "X" } as const;
    expect(leafHeight({ ...plain, description: "d".repeat(100) })).toBeGreaterThan(leafHeight(plain));
  });

  const BUSY = `C4Container
  Person(u, "User", "Uses it")
  System_Ext(ext, "Outside", "Another system")
  System_Boundary(s, "System") {
    Container(a, "A", "Rust", "First app")
    Container(b, "B", "Rust", "Second app")
    ContainerDb(db, "DB", "SQL", "Keeps data")
  }
  Rel(u, a, "Opens the app and reads what it shows", "HTTPS")
  Rel(a, b, "Calls", "gRPC")
  Rel(a, db, "Reads and writes rows", "SQL")
  Rel(b, db, "Writes events", "SQL")
  Rel(a, ext, "Sends mail", "SMTP")
  Rel(ext, u, "Notifies")
`;

  it("sets a boundary's name in a corner that no arrow crosses", async () => {
    // Four people reach two apps inside the boundary, so arrows come down through its top left.
    const d = diagram(`C4Container
  Person(p1, "Admins", "Run it")
  Person(p2, "Tenants", "Watch it")
  Person(p3, "Workers", "Use it")
  System_Ext(gps, "GPS", "Devices")
  System_Boundary(s, "System") {
    Container(web, "Web", "React", "Browser app")
    Container(app, "App", "Flutter", "Mobile app")
    Container(api, "Api", "Rust", "Serves clients")
    ContainerDb(db, "DB", "SQL", "Keeps data")
  }
  System_Ext(geo, "Geocoder", "Addresses")
  System_Ext(idp, "Identity", "Sign-in")
  Rel(p1, web, "Administers", "HTTPS")
  Rel(p2, web, "Monitors", "HTTPS")
  Rel(p3, app, "Works in", "HTTPS")
  Rel(gps, api, "Sends telemetry", "TCP")
  Rel(web, api, "Reads", "HTTP")
  Rel(app, api, "Reads", "HTTP")
  Rel(api, db, "Stores", "SQL")
  Rel(api, geo, "Resolves", "HTTP")
  Rel(api, idp, "Proxies", "OIDC")
`);
    const geometry = await placeC4(d, true);
    const { nodes } = drawC4(d, geometry);
    const name = nodes.find((n) => n.id === "s#name");
    const boundary = geometry.boxes.get("s") as Box;
    const size = nameSize(d.elements.find((e) => e.alias === "s") as C4Element);
    const at = { x: boundary.x + (name?.position.x ?? 0), y: boundary.y + (name?.position.y ?? 0), ...size };
    for (const [rel, points] of geometry.routes) {
      points.slice(1).forEach((p, i) => {
        const q = points[i] as { x: number; y: number };
        const meets = Math.min(p.x, q.x) < at.x + at.width && at.x < Math.max(p.x, q.x) && Math.min(p.y, q.y) < at.y + at.height && at.y < Math.max(p.y, q.y);
        expect(meets, `arrow ${rel} crosses the name`).toBe(false);
      });
    }
  });

  it("keeps every label clear of every box, every other label and every other arrow", async () => {
    const d = diagram(BUSY);
    const geometry = await placeC4(d, true);
    const absolute = (alias: string): Box => {
      const box = geometry.boxes.get(alias) as Box;
      const parent = d.elements.find((e) => e.alias === alias)?.parent;
      const outer = parent === undefined ? { x: 0, y: 0 } : absolute(parent);
      return { ...box, x: box.x + outer.x, y: box.y + outer.y };
    };
    const meets = (a: Box, b: Box) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
    expect(geometry.labels.size).toBe(d.rels.length);
    for (const [at, label] of geometry.labels) {
      for (const element of d.elements.filter((e) => e.kind !== "boundary")) {
        expect(meets(label, absolute(element.alias)), `label ${at} on ${element.alias}`).toBe(false);
      }
      for (const [other, box] of geometry.labels) if (other !== at) expect(meets(label, box), `label ${at} on label ${other}`).toBe(false);
      for (const [other, points] of geometry.routes) {
        if (other === at) continue;
        points.slice(1).forEach((p, i) => {
          const q = points[i] as { x: number; y: number };
          const segment = { x: Math.min(p.x, q.x), y: Math.min(p.y, q.y), width: Math.abs(p.x - q.x), height: Math.abs(p.y - q.y) };
          expect(meets(label, segment), `label ${at} on arrow ${other}`).toBe(false);
        });
      }
    }
  });

  it("a selection fades what is not connected to it", async () => {
    const { nodes, edges } = await layoutC4(diagram(SHOP), "db");
    const faded = nodes.filter((n) => n.data.faded).map((n) => n.id);
    expect(faded.sort()).toEqual(["bus", "buyer", "pay"]);
    expect(edges.find((e) => e.target === "db")?.style?.opacity).toBe(1);
    expect(edges.find((e) => e.source === "buyer")?.style?.opacity).toBe(0.15);
  });
});

describe("C4 diagrams in a doc", () => {
  const resolver = buildResolver([listRow("arch/api")]);

  it("draws a C4 fence with stone's viewer and opens an element's details on click", async () => {
    render(<Markdown source={`# Map\n\n\`\`\`mermaid\n${SHOP}\`\`\`\n`} resolver={resolver} />);
    const figure = await screen.findByRole("figure", { name: "Shop containers" });
    expect(mermaid.render).not.toHaveBeenCalled();
    expect(await within(figure).findByText("Orders DB")).toBeInTheDocument();
    expect(within(figure).getByText("2 lines were not drawn")).toBeInTheDocument();

    fireEvent.click(within(figure).getByText("API"));
    const panel = await screen.findByRole("complementary", { name: "API details" });
    expect(panel).toHaveTextContent("Serves every client");
    expect(within(panel).getByRole("link", { name: "Open arch/api" })).toHaveAttribute("href", "/items/arch/api");
    const rels = within(panel).getByRole("region", { name: "Relationships" });
    expect(rels).toHaveTextContent("Orders DB: Reads and writes [SQL]");
    expect(rels).toHaveTextContent("Buyer: Places orders [HTTPS]");

    // A relationship's other end is one click away.
    await userEvent.click(within(rels).getByRole("button", { name: "Orders DB" }));
    expect(await screen.findByRole("complementary", { name: "Orders DB details" })).toHaveTextContent("Keeps orders");
  });

  it("other Mermaid fences still go to Mermaid", async () => {
    mermaid.render.mockResolvedValue({ svg: '<svg data-testid="diagram"></svg>' });
    render(<Markdown source={"```mermaid\ngraph TD\n  A --> B\n```\n"} resolver={resolver} />);
    expect(await screen.findByTestId("diagram")).toBeInTheDocument();
  });
});
