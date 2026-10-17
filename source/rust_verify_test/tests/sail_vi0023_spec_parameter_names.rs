#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// VI-0023: a source spelling change to one spec formal must not change the
// definition axioms imported by an otherwise untouched caller module.
// PARAM marks only the outer formal/references. A literal `sail_arg` below
// can therefore shadow the original formal without being renamed itself.
fn client_smt(definitions: &str, caller: &str, renamed: bool) -> String {
    assert!(definitions.contains("PARAM"));
    let definitions = definitions.replace("PARAM", if renamed { "sail_new" } else { "sail_arg" });
    let code = format!(
        "::verus_builtin_macros::verus! {{ {definitions}\nmod client {{ use vstd::prelude::*; {caller} }} }}"
    );
    let logs = tempfile::tempdir().expect("SMT log directory");
    let log_dir = format!("--sail-raw={}", logs.path().display());
    let options = [
        "vstd",
        "--crate-name=sail_vi0023",
        "--sail-raw=--log",
        "--sail-raw=smt",
        "--sail-raw=--log-dir",
        log_dir.as_str(),
    ];
    let result = verify_one_file("sail_vi0023_spec_parameter_names", code, &options);
    assert!(result.is_ok(), "verification failed: {:?}", result);
    let mut files: Vec<_> = std::fs::read_dir(logs.path())
        .expect("SMT log directory")
        .map(|entry| entry.expect("SMT log entry").path())
        .filter(|path| {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
            name.starts_with("client") && name.ends_with(".smt2")
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "missing caller SMT logs");
    let mut result = String::new();
    for file in files {
        let text = std::fs::read_to_string(file).expect("SMT log");
        // These are log-only source-position comments, as in VI-0019's test.
        // No solver-input token is renamed or normalized by this comparison.
        for line in text.lines().filter(|line| !line.trim_start().starts_with(";;")) {
            result.push_str(line);
            result.push('\n');
        }
    }
    assert!(result.contains("(check-sat)"), "caller has no solver check");
    assert!(result.contains("spec%param%"), "stable spec formals are not exercised");
    result
}

fn unchanged_caller(definitions: &str, caller: &str) {
    let before = client_smt(definitions, caller, false);
    let after = client_smt(definitions, caller, true);
    assert_eq!(before, after, "renaming a spec formal changed caller SMT");
}

#[test]
fn sail_vi0023_spec_parameter_rename_keeps_caller_axioms() {
    unchanged_caller(
        r#"mod defs {
            use vstd::prelude::*;
            pub open spec fn value(PARAM: int, unused: int, right: int) -> int {
                PARAM - right
            }
        }"#,
        r#"proof fn check(x: int, y: int) {
            assert(crate::defs::value(x, 91, y) == x - y);
        }"#,
    );
}

#[test]
fn sail_vi0023_shadowing_and_unused_positions_do_not_capture() {
    unchanged_caller(
        r#"mod defs {
            use vstd::prelude::*;
            pub open spec fn value(PARAM: int, unused: int, right: int) -> int {
                let saved = PARAM;
                let sail_arg = right + 1;
                saved + sail_arg
            }
        }"#,
        r#"proof fn check(x: int, y: int) {
            assert(crate::defs::value(x, 91, y) == x + y + 1);
        }"#,
    );
}

#[test]
fn sail_vi0023_recursive_parameter_copies_keep_positions() {
    unchanged_caller(
        r#"mod defs {
            use vstd::prelude::*;
            pub open spec fn value(PARAM: nat) -> nat decreases PARAM {
                if PARAM == 0 { 0 } else { 1 + value((PARAM - 1) as nat) }
            }
        }"#,
        r#"proof fn check() {
            assert(crate::defs::value(1) == 1);
        }"#,
    );
}

#[test]
fn sail_vi0023_generic_trait_default_callers_keep_spec_arguments() {
    unchanged_caller(
        r#"mod defs {
            use vstd::prelude::*;
            pub open spec fn value<T>(PARAM: T, unused: T) -> T { PARAM }
            pub trait Pick {
                fn copy<T>(value: T) -> (result: T)
                    ensures result == crate::defs::value(value, value)
                { value }
            }
            pub struct Token;
            impl Pick for Token { }
        }"#,
        r#"fn check() -> (result: u8) ensures result == 3 {
            <crate::defs::Token as crate::defs::Pick>::copy(3u8)
        }"#,
    );
}

#[test]
fn sail_vi0023_parameter_rename_does_not_hide_counterexamples() {
    let mut diagnostics = Vec::new();
    for parameter in ["sail_arg", "sail_new"] {
        let code = format!(
            r#"::verus_builtin_macros::verus! {{
                mod defs {{
                    use vstd::prelude::*;
                    pub open spec fn value({parameter}: int, unused: int, right: int) -> int {{
                        {parameter} - right
                    }}
                }}
                mod client {{
                    use vstd::prelude::*;
                    proof fn bad(x: int, y: int) {{
                        assert(crate::defs::value(x, 91, y) == x + y);
                    }}
                }}
            }}"#,
        );
        let result = verify_one_file("sail_vi0023_counterexample", code, &["vstd"]);
        let err = result.expect_err("false client assertion must fail");
        assert_eq!(err.errors.len(), 1, "{:?}", err.errors);
        assert!(err.errors[0].message.contains("assertion failed"));
        diagnostics.push((
            err.errors[0].message.clone(),
            relevant_error_span(&err.errors[0].spans).line_start,
        ));
    }
    assert_eq!(diagnostics[0], diagnostics[1], "unchanged caller diagnostic changed");
}
