//! Dev-only in-process unit-test runner with a hand-maintained registry.
//!
//! `unittest_test!` emits a `#[test]` wrapper (so `cargo test` still finds and
//! runs it) and a plain `pub fn` (so the registry below can reference it). The
//! tests are listed explicitly in `TEST_SUITES` - an array of (module, tests)
//! groups. Each entry references its test function directly, which makes the
//! linker pull the containing object from the Rust `staticlib` when building
//! the `.so` (no `--whole-archive` / `linkme` needed). `run_all()` iterates
//! the registry and executes each test in-process, catching panics so one
//! failure does not abort the whole suite. There is no shelling out to
//! `cargo test`.

#![allow(dead_code)]

#[cfg(feature = "dev")]
use crate::beprintln;

/// A single unit test: a name plus a parameterless function that panics
/// (`assert!`/regular panic) on failure.
pub type UnitTest = fn();

pub struct TestCase {
    pub name: &'static str,
    pub run: UnitTest,
}

/// Manual registry of all unit tests, grouped by module. Add a test with
/// `unittest_test!` and list it here.
#[cfg(feature = "dev")]
pub static TEST_SUITES: &[(&str, &[TestCase])] = &[
    (
        "bash_api",
        &[
            TestCase {
                name: "test_valid_var_names",
                run: crate::bash_api::tests::test_valid_var_names,
            },
            TestCase {
                name: "test_invalid_var_names",
                run: crate::bash_api::tests::test_invalid_var_names,
            },
        ],
    ),
    (
        "bprint_bytes",
        &[
            TestCase {
                name: "test_basic",
                run: crate::bprint_bytes::tests::test_basic,
            },
            TestCase {
                name: "test_cstr",
                run: crate::bprint_bytes::tests::test_cstr,
            },
            TestCase {
                name: "test_multi",
                run: crate::bprint_bytes::tests::test_multi,
            },
            TestCase {
                name: "test_stderr",
                run: crate::bprint_bytes::tests::test_stderr,
            },
            TestCase {
                name: "test_bwriteln",
                run: crate::bprint_bytes::tests::test_bwriteln,
            },
        ],
    ),
];

/// Run every registered test, catching panics, and return the failure count.
/// A per-test status and a summary line are printed to stderr.
#[cfg(feature = "dev")]
pub fn run_all() -> usize {
    let mut failed = 0usize;
    for (group, tests) in TEST_SUITES.iter() {
        beprintln!(group.as_bytes(), b":");
        for tc in tests.iter() {
            beprintln!(b"  test ", tc.name.as_bytes(), b" ...");
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (tc.run)())) {
                Ok(()) => {
                    beprintln!(b"    ok");
                }
                Err(_) => {
                    failed += 1;
                    beprintln!(b"    FAILED (panic)");
                }
            }
        }
    }
    let total: usize = TEST_SUITES.iter().map(|(_, t)| t.len()).sum();
    let summary = format!("test result: {} passed; {} failed", total - failed, failed);
    beprintln!(summary.as_bytes());
    failed
}

/// Define a unit test that works both as a `cargo test` `#[test]` and as a
/// registry entry for the in-process `unittest` subcommand.
///
/// - under `cargo test`: emitted as a `#[test]` function.
/// - under the `dev` feature: emitted as a plain `pub fn` (listed in
///   `TEST_SUITES` so `unittest` can run it).
/// - under both: a single `#[test]` function (no duplicate-symbol clash).
/// - under neither: nothing is emitted (release `.so` carries no tests).
#[macro_export]
macro_rules! unittest_test {
    (fn $name:ident() $body:block) => {
        #[cfg(any(feature = "dev", test))]
        #[cfg_attr(test, test)]
        pub fn $name() {
            $body
        }
    };
}
