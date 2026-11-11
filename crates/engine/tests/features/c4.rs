//! C4: the context, container and component diagrams drawn from arch items.

use carvestone_engine::Repo;
use carvestone_engine::arch_c4::{c4_component, c4_container, c4_context};

use crate::support::Store;

fn item(store: &Store, id: &str, front: &str, body: &str) {
    store.write(&format!("stone/{id}.md"), &format!("---\n{front}\n---\n\n{body}"));
}

/// A product of two child systems, a person, an outside system and a store. The buyer reaches the
/// site, the site calls the api and the payments, the api writes the store and the payments, and the
/// api's orders component writes the store too. `routes` calls `orders` in code only.
fn shop() -> Store {
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    item(
        &store,
        "arch/shop",
        "type: system\ntitle: Shop\ndescription: Sells things online.",
        "",
    );
    item(
        &store,
        "arch/shop/web",
        "type: system\ntitle: Web shop\nparent: arch/shop\ndescription: Takes the orders.",
        "",
    );
    item(
        &store,
        "arch/shop/back",
        "type: system\ntitle: Back office\nparent: arch/shop",
        "",
    );
    item(
        &store,
        "arch/shop/web/site",
        "type: container\ntitle: Site\nparent: arch/shop/web\ntechnology: React\ndescription: Shows the shop.\nuses: [arch/shop/back/api, arch/pay]",
        "## Uses\n\n- [[arch/shop/back/api]]: Places orders (HTTP)\n- [[arch/pay]]: Sends card details (HTTP)\n",
    );
    item(
        &store,
        "arch/shop/back/api",
        "type: container\ntitle: Api\nparent: arch/shop/back\ntechnology: Rust\ndescription: Serves orders.\nuses: [arch/shop/back/db, arch/pay]\ncode: [src/lib.rs]",
        "## Uses\n\n- [[arch/shop/back/db]]: Saves orders (SQL)\n- [[arch/pay]]: Charges cards (HTTPS)\n",
    );
    item(
        &store,
        "arch/shop/back/db",
        "type: container\ntitle: Orders DB\nparent: arch/shop/back\nkind: store\ntechnology: PostgreSQL\ndescription: Keeps every order.",
        "",
    );
    item(
        &store,
        "arch/shop/back/api/orders",
        "type: component\ntitle: Orders\nparent: arch/shop/back/api\nuses: [arch/shop/back/db]\ncode: [src/orders.rs]",
        "## Uses\n\n- [[arch/shop/back/db]]: Writes rows (SQL)\n",
    );
    item(
        &store,
        "arch/shop/back/api/routes",
        "type: component\ntitle: Routes\nparent: arch/shop/back/api\ndescription: Maps URLs.\ncode: [src/routes.rs]",
        "",
    );
    item(
        &store,
        "arch/buyer",
        "type: system\ntitle: Buyer\nkind: person\ndescription: Buys things.\nuses: [arch/shop/web/site]",
        "## Uses\n\n- [[arch/shop/web/site]]: Browses items (HTTPS)\n",
    );
    item(
        &store,
        "arch/pay",
        "type: system\ntitle: Payments\nkind: external\ndescription: Takes the money.",
        "",
    );
    store.write("Cargo.toml", "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n");
    store.write("src/lib.rs", "pub mod orders;\npub mod routes;\n");
    store.write("src/orders.rs", "pub fn place() -> u32 {\n    1\n}\n");
    store.write(
        "src/routes.rs",
        "use crate::orders::place;\n\npub fn post() -> u32 {\n    place()\n}\n",
    );
    store.git_track();
    store
}

/// What the shop draws at each level, written by hand.
const CONTEXT: &str = r#"C4Context
  title System context diagram for Shop
  Person(arch_buyer, "Buyer", "Buys things.", $link="arch/buyer")
  System_Ext(arch_pay, "Payments", "Takes the money.", $link="arch/pay")
  System(arch_shop, "Shop", "Sells things online.", $link="arch/shop")
  Rel(arch_buyer, arch_shop, "Browses items", "HTTPS")
  Rel(arch_shop, arch_pay, "Charges cards, Sends card details", "HTTPS, HTTP")
"#;

const CONTAINER: &str = r#"C4Container
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
"#;

const COMPONENT: &str = r##"C4Component
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
"##;

#[test]
fn each_level_draws_the_boxes_it_owns_and_lifts_the_connections_of_what_sits_inside() {
    let store = shop();
    let repo = store.repo();

    // The site and the api sit in one product, so the line between them is inside one box; the
    // two lines to the payments become one.
    assert_eq!(c4_context(&repo).unwrap(), CONTEXT);

    // The orders component writes the store, but the line the api itself has to it wins over the one
    // lifted from the component, and the routes-to-orders line is inside the api.
    assert_eq!(c4_container(&repo, None).unwrap(), CONTAINER);
    assert_eq!(
        c4_container(&repo, Some("arch/shop")).unwrap(),
        c4_container(&repo, None).unwrap()
    );

    // The routes component reaches the orders component only in code: two edges, no written line.
    assert_eq!(c4_component(&repo, "arch/shop/back/api").unwrap(), COMPONENT);

    // Same store, same text.
    assert_eq!(
        c4_component(&repo, "arch/shop/back/api").unwrap(),
        c4_component(&repo, "arch/shop/back/api").unwrap()
    );
}

fn arch_file(id: &str, front: &str, body: &str) -> (String, String) {
    (format!("stone/{id}.md"), format!("---\n{front}\n---\n\n{body}"))
}

/// Files written over the shop: (path, text).
type Files = Vec<(String, String)>;

type Draw = fn(&Repo) -> Result<String, carvestone_engine::EngineError>;

#[test]
fn a_connection_with_no_phrase_never_hides_one_and_a_line_inside_its_own_box_is_dropped() {
    let api = "type: container\ntitle: Api\nparent: arch/shop/back\ntechnology: Rust\ndescription: Serves orders.\ncode: [src/lib.rs]";
    let orders = "type: component\ntitle: Orders\nparent: arch/shop/back/api\ncode: [src/orders.rs]";
    let context: Draw = c4_context;
    let container: Draw = |repo| c4_container(repo, None);
    let component: Draw = |repo| c4_component(repo, "arch/shop/back/api");
    // (case, files written over the shop, the diagram, the text it must still be)
    let cases: [(&str, Files, Draw, String); 6] = [
        (
            "the product lists the payments with no phrase, and the lifted phrases stay",
            vec![arch_file(
                "arch/shop",
                "type: system\ntitle: Shop\ndescription: Sells things online.\nuses: [arch/pay]",
                "",
            )],
            context,
            CONTEXT.to_string(),
        ),
        (
            "routes lists orders with no phrase, and the code references still name the line",
            vec![arch_file(
                "arch/shop/back/api/routes",
                "type: component\ntitle: Routes\nparent: arch/shop/back/api\ndescription: Maps URLs.\nuses: [arch/shop/back/api/orders]\ncode: [src/routes.rs]",
                "",
            )],
            component,
            COMPONENT.to_string(),
        ),
        (
            "the api reports to the shop it sits in",
            vec![arch_file(
                "arch/shop/back/api",
                &format!("{api}\nuses: [arch/shop/back/db, arch/pay, arch/shop]"),
                "## Uses\n\n- [[arch/shop/back/db]]: Saves orders (SQL)\n- [[arch/pay]]: Charges cards (HTTPS)\n- [[arch/shop]]: Reports health (HTTP)\n",
            )],
            container,
            CONTAINER.to_string(),
        ),
        (
            "the orders component reports to the api it sits in",
            vec![arch_file(
                "arch/shop/back/api/orders",
                &format!("{orders}\nuses: [arch/shop/back/db, arch/shop/back/api]"),
                "## Uses\n\n- [[arch/shop/back/db]]: Writes rows (SQL)\n- [[arch/shop/back/api]]: Reports health (HTTP)\n",
            )],
            component,
            COMPONENT.to_string(),
        ),
        (
            "one code edge reads as one reference",
            vec![(
                "src/routes.rs".to_string(),
                "pub fn post() -> u32 {\n    crate::orders::place()\n}\n".to_string(),
            )],
            component,
            COMPONENT.replace("2 references", "1 reference"),
        ),
        (
            "a connection with no phrase and nothing lifted into it keeps an empty label",
            vec![arch_file(
                "arch/pay",
                "type: system\ntitle: Payments\nkind: external\ndescription: Takes the money.\nuses: [arch/buyer]",
                "",
            )],
            context,
            CONTEXT.replace(
                "  Rel(arch_shop, arch_pay",
                "  Rel(arch_pay, arch_buyer, \"\")\n  Rel(arch_shop, arch_pay",
            ),
        ),
    ];
    for (what, files, draw, want) in cases {
        let store = shop();
        for (path, text) in &files {
            store.write(path, text);
        }
        assert_eq!(draw(&store.repo()).unwrap(), want, "{what}");
    }
}

fn systems() -> Store {
    let store = Store::new();
    item(&store, "arch/a", "type: system\ntitle: A\nparent: arch/b", "");
    item(&store, "arch/b", "type: system\ntitle: B\nparent: arch/a", "");
    item(&store, "arch/c", "type: system\ntitle: C\nparent: arch/c", "");
    store
}

fn quoted() -> Store {
    let store = Store::new();
    item(
        &store,
        "arch/q",
        "type: system\ntitle: The \"Shop\"\ndescription: Says \"hi\".",
        "",
    );
    store
}

#[test]
fn a_parent_loop_several_products_an_unknown_id_and_a_quote_each_get_an_answer() {
    let cases: [(&str, Store, Draw, Result<&str, &str>); 5] = [
        (
            "a parent loop ends at its smallest id; a system that is its own parent is a product",
            systems(),
            c4_context,
            Ok(
                "C4Context\n  title System landscape diagram\n  System(arch_a, \"A\", $link=\"arch/a\")\n  System(arch_c, \"C\", $link=\"arch/c\")\n",
            ),
        ),
        (
            "two products and no system named",
            systems(),
            |repo| c4_container(repo, None),
            Err("this store has several systems: arch/a, arch/c; pass one"),
        ),
        (
            "a system that is not there",
            systems(),
            |repo| c4_container(repo, Some("arch/nope")),
            Err("`arch/nope` is not a system with no kind; the systems are: arch/a, arch/b, arch/c"),
        ),
        (
            "a container that is not there",
            systems(),
            |repo| c4_component(repo, "arch/nope"),
            Err("`arch/nope` is not a container, and this store has no containers; add one with `stone new container`"),
        ),
        (
            "a double quote in a value",
            quoted(),
            c4_context,
            Ok(
                "C4Context\n  title System context diagram for The \"Shop\"\n  System(arch_q, \"The #quot;Shop#quot;\", \"Says #quot;hi#quot;.\", $link=\"arch/q\")\n",
            ),
        ),
    ];
    for (what, store, draw, want) in cases {
        let got = draw(&store.repo()).map_err(|e| e.to_string());
        assert_eq!(got, want.map(String::from).map_err(String::from), "{what}");
    }
}
