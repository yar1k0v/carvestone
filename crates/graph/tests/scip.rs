//! Reading SCIP symbol strings as declarations of ours.

use carvestone_graph::{ScipSymbol, scip_symbol};

/// `(symbol, expected)`; the expected values follow the SCIP symbol grammar
/// (`scheme manager package version descriptors`, `/` namespace, `#` type, `.` term, `().` method).
#[test]
fn a_scip_symbol_reads_as_a_declaration_or_is_refused() {
    let found = |qname: &str, name: &str, kind: &'static str, parent: Option<&str>| {
        Some(ScipSymbol {
            qname: qname.into(),
            name: name.into(),
            kind,
            parent: parent.map(Into::into),
        })
    };
    let rows = [
        (
            "semanticdb maven com.example 1.0 com/example/Greeter#",
            found("com.example.Greeter", "Greeter", "type", Some("com.example")),
        ),
        (
            "semanticdb maven com.example 1.0 com/example/Greeter#greet().",
            found(
                "com.example.Greeter.greet",
                "greet",
                "method",
                Some("com.example.Greeter"),
            ),
        ),
        (
            "semanticdb maven com.example 1.0 com/example/Greeter#name.",
            found(
                "com.example.Greeter.name",
                "name",
                "member",
                Some("com.example.Greeter"),
            ),
        ),
        (
            "scip-go gomod github.com/acme/app 1.0.0 `github.com/acme/app/util`/Helper().",
            found(
                "github.com/acme/app/util.Helper",
                "Helper",
                "function",
                Some("github.com/acme/app/util"),
            ),
        ),
        (
            "scip-go gomod github.com/acme/app 1.0.0 `github.com/acme/app/util`/",
            found("github.com/acme/app/util", "github.com/acme/app/util", "module", None),
        ),
        // A parameter is not a declaration of ours.
        (
            "semanticdb maven com.example 1.0 com/example/Greeter#greet().(who)",
            None,
        ),
        ("local 12", None),
        ("not a symbol", None),
    ];
    for (symbol, want) in rows {
        assert_eq!(scip_symbol(symbol), want, "{symbol}");
    }
}
