use std::path::Path;

use rust_anything::analyze::{Bridge, Report, Status, UnitReport, analyze};
use rust_anything::risks::Risk;

fn mini() -> Report {
    analyze(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")).unwrap()
}

fn unit<'r>(report: &'r Report, id: &str) -> &'r UnitReport {
    report
        .units
        .iter()
        .find(|u| u.id == id)
        .unwrap_or_else(|| panic!("no unit {id}"))
}

#[test]
fn groups_files_into_units_and_skips_fixtures() {
    let report = mini();
    let ids: Vec<&str> = report.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "packages/a/src/fs.ts",
            "packages/a/src/parse/",
            "packages/a/src/svelte.ts",
            "packages/a/src/util.ts",
            "packages/a/src/view/",
            "packages/b/src/index.ts",
        ]
    );
    assert_eq!(unit(&report, "packages/a/src/parse/").files.len(), 2);
    assert_eq!(report.summary.test_files, 1);
}

#[test]
fn resolves_imports_into_waves_and_credits_tests() {
    let report = mini();
    let util = unit(&report, "packages/a/src/util.ts");
    assert_eq!(util.wave, 0);
    assert_eq!(util.direct_tests, 1);
    assert_eq!(util.fan_in, 2);
    let fs = unit(&report, "packages/a/src/fs.ts");
    assert_eq!(fs.wave, 1);
    assert_eq!(fs.internal_deps, ["packages/a/src/util.ts"]);
    assert_eq!(
        unit(&report, "packages/b/src/index.ts").workspace_deps,
        ["a"]
    );
}

#[test]
fn finds_risks_with_locations() {
    let report = mini();
    let util = unit(&report, "packages/a/src/util.ts");
    let regex = util
        .risks
        .iter()
        .find(|r| r.risk == Risk::RegexSyntax)
        .unwrap();
    assert_eq!(regex.samples, ["packages/a/src/util.ts:1"]);
    assert!(util.risks.iter().any(|r| r.risk == Risk::StringOffsets));
}

#[test]
fn picks_status_and_bridge() {
    let report = mini();
    let fs = unit(&report, "packages/a/src/fs.ts");
    assert_eq!((fs.status, fs.bridge), (Status::Ready, Bridge::NapiRs));
    let view = unit(&report, "packages/a/src/view/");
    assert_eq!((view.status, view.bridge), (Status::Keep, Bridge::KeepInJs));
    assert_eq!(
        unit(&report, "packages/a/src/svelte.ts").status,
        Status::Blocked
    );
    // `b` uses no Node API, so its code can run anywhere.
    assert_eq!(
        unit(&report, "packages/b/src/index.ts").bridge,
        Bridge::WasmBindgen
    );
    let order: Vec<(&str, usize)> = report
        .packages
        .iter()
        .map(|p| (p.name.as_str(), p.order))
        .collect();
    assert_eq!(order, [("a", 1), ("b", 2)]);
    assert!(!report.start_here.is_empty());
    assert!(
        !report
            .start_here
            .iter()
            .any(|id| id.ends_with("svelte.ts") || id.ends_with("view/"))
    );
}

#[test]
fn json_output_is_versioned() {
    let json = serde_json::to_value(mini()).unwrap();
    assert_eq!(json["schema"], "rust-anything/analyze@1");
    assert_eq!(json["units"][0]["status"], "ready");
    assert_eq!(json["units"][0]["bridge"], "napi-rs");
}
