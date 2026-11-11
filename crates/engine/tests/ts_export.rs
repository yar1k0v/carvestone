//! Writes the TypeScript bindings of every API struct to `web/src/api/types/`. CI runs this and
//! fails on `git diff` of that folder, so a changed struct cannot ship with stale types.
#![cfg(feature = "ts")]

use std::path::PathBuf;

use carvestone_engine::activity::ActivityGroup;
use carvestone_engine::api::{
    CodeEvent, CreateReply, CreateRequest, ErrorBody, ErrorEvent, ItemsEvent, PatchReply, PatchRequest,
};
use carvestone_engine::api_config::ApiConfig;
use carvestone_engine::arch_c4::C4Source;
use carvestone_engine::arch_graph::ArchGraph;
use carvestone_engine::changed_files::ChangedFile;
use carvestone_engine::check::Problem;
use carvestone_engine::code::{CodeResult, Outline};
use carvestone_engine::code_domain::DomainModel;
use carvestone_engine::code_graph::{Explain, Impact, PathResult};
use carvestone_engine::code_native::NativeStatus;
use carvestone_engine::code_symbol::SymbolPage;
use carvestone_engine::design::{Hld, Lld};
use carvestone_engine::docs_list::DocRow;
use carvestone_engine::history::{History, HistoryEntry};
use carvestone_engine::modules::Module;
use carvestone_engine::product::{FeatureDetail, Product, TaskFeature};
use carvestone_engine::query::{GetResult, ListRow};
use carvestone_engine::search::SearchResult;
use carvestone_engine::stale::Stale;
use carvestone_engine::timeline::{Event, EventKind};
use carvestone_engine::why::{CommitRef, Related, Why};
use carvestone_engine::work_links::TaskCommit;
use ts_rs::{Config, TS};

fn types_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/src/api/types")
}

#[test]
fn ts_export() {
    let dir = types_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = Config::new().with_out_dir(&dir);
    // Each call also writes the types its argument is built from.
    ListRow::export_all(&cfg).unwrap();
    GetResult::export_all(&cfg).unwrap();
    SearchResult::export_all(&cfg).unwrap();
    CodeResult::export_all(&cfg).unwrap();
    Outline::export_all(&cfg).unwrap();
    SymbolPage::export_all(&cfg).unwrap();
    TaskFeature::export_all(&cfg).unwrap();
    DomainModel::export_all(&cfg).unwrap();
    Impact::export_all(&cfg).unwrap();
    PathResult::export_all(&cfg).unwrap();
    Explain::export_all(&cfg).unwrap();
    Module::export_all(&cfg).unwrap();
    ApiConfig::export_all(&cfg).unwrap();
    ArchGraph::export_all(&cfg).unwrap();
    C4Source::export_all(&cfg).unwrap();
    Hld::export_all(&cfg).unwrap();
    Lld::export_all(&cfg).unwrap();
    DocRow::export_all(&cfg).unwrap();
    Product::export_all(&cfg).unwrap();
    FeatureDetail::export_all(&cfg).unwrap();
    ActivityGroup::export_all(&cfg).unwrap();
    TaskCommit::export_all(&cfg).unwrap();
    Problem::export_all(&cfg).unwrap();
    PatchRequest::export_all(&cfg).unwrap();
    PatchReply::export_all(&cfg).unwrap();
    CreateRequest::export_all(&cfg).unwrap();
    CreateReply::export_all(&cfg).unwrap();
    ErrorBody::export_all(&cfg).unwrap();
    ItemsEvent::export_all(&cfg).unwrap();
    CodeEvent::export_all(&cfg).unwrap();
    ErrorEvent::export_all(&cfg).unwrap();
    ChangedFile::export_all(&cfg).unwrap();
    Stale::export_all(&cfg).unwrap();
    Why::export_all(&cfg).unwrap();
    Related::export_all(&cfg).unwrap();
    CommitRef::export_all(&cfg).unwrap();
    History::export_all(&cfg).unwrap();
    HistoryEntry::export_all(&cfg).unwrap();
    Event::export_all(&cfg).unwrap();
    EventKind::export_all(&cfg).unwrap();
    NativeStatus::export_all(&cfg).unwrap();

    for name in [
        "ListRow",
        "GetResult",
        "ItemView",
        "Trust",
        "Section",
        "ApiConfig",
        "ArchGraph",
        "Hld",
        "Lld",
        "DocRow",
    ] {
        assert!(dir.join(format!("{name}.ts")).is_file(), "{name}.ts was not written");
    }

    // The client reads `etag` off the whole union, so every member must declare it.
    for name in ["ItemView", "SectionView", "CodeView"] {
        let text = std::fs::read_to_string(dir.join(format!("{name}.ts"))).unwrap();
        assert!(text.contains("etag"), "{name}.ts has no etag");
    }
}
