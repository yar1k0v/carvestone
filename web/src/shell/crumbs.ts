import type { Crumb } from "./pageFrame";

type TitleOf = (id: string) => string | undefined;

const WORK_STORES = new Set(["tasks", "plans", "milestones"]);

function architecture(tail: string, titleOf: TitleOf): Crumb[] {
  const parts = tail.split("/");
  const below = parts.flatMap((part, at): Crumb[] => {
    const id = parts.slice(0, at + 1).join("/");
    // `arch` alone is no element; the trail starts below it.
    return id === "arch" ? [] : [{ label: titleOf(id) ?? part, href: `/architecture/${id}` }];
  });
  return [{ label: "System architecture", href: "/architecture" }, ...below];
}

function workSection(label: string, href: string, tail: string, titleOf: TitleOf): Crumb[] {
  if (tail === "") return [{ label: "Now" }, { label }];
  return [{ label: "Now" }, { label, href }, { label: titleOf(`${href.slice(1)}/${tail}`) ?? titleOf(tail) ?? tail }];
}

function item(tail: string, titleOf: TitleOf): Crumb[] {
  const store = tail.split("/")[0] ?? "";
  const label = titleOf(tail) ?? tail;
  if (WORK_STORES.has(store)) return [{ label: "Now" }, { label }];
  if (store === "arch") return [{ label: "System", href: "/architecture" }, { label }];
  return [{ label: "Library", href: "/docs" }, { label }];
}

function trailOf(root: string, tail: string, titleOf: TitleOf): Crumb[] {
  switch (root) {
    case "":
      return [{ label: "Now" }];
    case "product":
      return [{ label: "Product" }];
    case "features":
      return [{ label: "Product", href: "/product" }, { label: titleOf(`features/${tail}`) ?? tail }];
    case "architecture":
      return tail === "" ? [{ label: "System" }] : architecture(tail, titleOf);
    case "docs":
      return tail === "" ? [{ label: "Library" }] : [{ label: "Library", href: "/docs" }, { label: titleOf(`docs/${tail}`) ?? tail }];
    case "items":
      return item(tail, titleOf);
    case "milestones":
      return workSection("Milestones", "/milestones", tail, titleOf);
    case "plans":
      return workSection("Plans", "/plans", tail, titleOf);
    case "board":
      return [{ label: "Now" }, { label: "Board" }];
    case "activity":
      return [{ label: "Now" }, { label: "Shipped" }];
    case "search":
      return [{ label: "Search" }];
    default:
      return [{ label: root }];
  }
}

/** The trail for an address; `titleOf` turns a path id or short id into its title. The last crumb is the page. */
export function crumbsFor(pathname: string, titleOf: TitleOf): Crumb[] {
  const [root = "", ...rest] = decodeURI(pathname).replace(/\/+$/, "").split("/").slice(1);
  const trail = trailOf(root, rest.join("/"), titleOf);
  // The page itself is not a link.
  return trail.map((crumb, at) => (at === trail.length - 1 ? { label: crumb.label } : crumb));
}
