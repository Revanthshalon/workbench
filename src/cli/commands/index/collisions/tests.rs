use super::{Collision, ModuleFile, losers};

fn file(path: &str, module: &str) -> ModuleFile {
    ModuleFile {
        path: path.to_owned(),
        module: module.to_owned(),
        stub: path.ends_with(".pyi"),
    }
}

const PYTHON: [&str; 2] = ["py", "pyi"];

#[test]
fn a_package_wins_over_a_module() {
    let found = losers(
        &[
            file("pkg/x.py", "pkg.x"),
            file("pkg/x/__init__.py", "pkg.x"),
        ],
        &PYTHON,
        false,
    );
    assert_eq!(
        found,
        [Collision {
            module: "pkg.x".to_owned(),
            winner: "pkg/x/__init__.py".to_owned(),
            losers: vec!["pkg/x.py".to_owned()],
        }]
    );
}

#[test]
fn a_plain_src_file_wins_over_a_root_script() {
    let found = losers(&[file("x.py", "x"), file("src/x.py", "x")], &PYTHON, false);
    assert_eq!(found[0].winner, "src/x.py");
    assert_eq!(found[0].losers, ["x.py"]);
}

#[test]
fn earlier_extensions_win_then_paths() {
    let found = losers(
        &[file("a.js", "a"), file("a.tsx", "a"), file("a.ts", "a")],
        &["ts", "tsx", "js"],
        false,
    );
    assert_eq!(found[0].winner, "a.ts");
    assert_eq!(found[0].losers, ["a.tsx", "a.js"]);
}

#[test]
fn a_stub_package_wins_over_a_stub_module() {
    let found = losers(
        &[
            file("pkg/x.pyi", "pkg.x"),
            file("pkg/x/__init__.pyi", "pkg.x"),
        ],
        &PYTHON,
        false,
    );
    assert_eq!(found[0].winner, "pkg/x/__init__.pyi");
    assert_eq!(found[0].losers, ["pkg/x.pyi"]);
}

#[test]
fn stubs_only_collide_with_stubs() {
    let found = losers(
        &[file("pkg/x.py", "pkg.x"), file("pkg/x.pyi", "pkg.x")],
        &PYTHON,
        false,
    );
    assert!(found.is_empty());
}
