#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0001, verifier side: under `cargo verus` the
// `verus_builtin_macros` crate is built from the project's dependency source (for
// example the stock crates.io release), so its `arrow_*` accessors arrive in
// `HashMap` order whatever this fork's macro does. The impl below is written
// exactly as stock's `enum_synthesize` emits it with the direct accessors in one
// of its random orders (fe, fc, fa, ...). The verifier must declare them in
// source order.

const ENUM_AND_STOCK_ORDER_IMPL: &str = code_str! {
    #[verus::internal(verus_macro)]
    pub enum E {
        A { fa: u8, fb: u8 },
        B { fc: u8, fd: u8 },
        C { fe: u8, ff: u8, fg: u8 },
    }

    #[verus::internal(verus_macro)]
    impl E {
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_fe(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "C")]); ::verus_builtin::get_variant_field(self, "C", "fe") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_fc(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "B")]); ::verus_builtin::get_variant_field(self, "B", "fc") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_fa(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "A")]); ::verus_builtin::get_variant_field(self, "A", "fa") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_fg(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "C")]); ::verus_builtin::get_variant_field(self, "C", "fg") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_fb(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "A")]); ::verus_builtin::get_variant_field(self, "A", "fb") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_ff(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "C")]); ::verus_builtin::get_variant_field(self, "C", "ff") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_fd(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "B")]); ::verus_builtin::get_variant_field(self, "B", "fd") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_A_fa(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "A")]); ::verus_builtin::get_variant_field(self, "A", "fa") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_A_fb(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "A")]); ::verus_builtin::get_variant_field(self, "A", "fb") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_B_fc(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "B")]); ::verus_builtin::get_variant_field(self, "B", "fc") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_B_fd(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "B")]); ::verus_builtin::get_variant_field(self, "B", "fd") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_C_fe(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "C")]); ::verus_builtin::get_variant_field(self, "C", "fe") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_C_ff(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "C")]); ::verus_builtin::get_variant_field(self, "C", "ff") }
        #[verus::internal(verus_macro)] #[verus::internal(spec)] #[verifier::inline] #[verus::internal(open)]
        pub fn arrow_C_fg(self) -> u8 { ::verus_builtin::recommends([::verus_builtin::is_variant(&self, "C")]); ::verus_builtin::get_variant_field(self, "C", "fg") }
    }
};

const USES: &str = verus_code_str! {
    pub open spec fn sum(e: E) -> int {
        match e {
            E::A { .. } => e->fa + e->fb,
            E::B { .. } => e->fc + e->fd,
            E::C { .. } => e->fe + e->ff + e->fg,
        }
    }
    proof fn uses(e: E) { assert(sum(e) == sum(e)); }
};

/// Verifies `code` with AIR logging and returns the positions of the first
/// mention of each `arrow_<field>` in the AIR log.
fn arrow_positions(name: &str, code: String, fields: &[&str]) -> Vec<usize> {
    let logs = tempfile::tempdir().expect("log directory");
    let log_dir = format!("--sail-raw={}", logs.path().display());
    let options = ["--sail-raw=--log", "--sail-raw=air", "--sail-raw=--log-dir", log_dir.as_str()];
    let result = verify_one_file(name, code, &options);
    assert!(result.is_ok(), "verification failed: {:?}", result);
    let air = std::fs::read_to_string(logs.path().join("root.air")).expect("root.air log");
    fields
        .iter()
        .map(|f| {
            air.find(&format!("arrow_{f}.")).unwrap_or_else(|| panic!("no arrow_{} in root.air", f))
        })
        .collect()
}

#[test]
fn sail_vi0001_stock_macro_accessors_declared_in_source_order() {
    let code = ENUM_AND_STOCK_ORDER_IMPL.to_string() + USES;
    let fields = ["fa", "fb", "fc", "fd", "fe", "ff", "fg"];
    let positions = arrow_positions("sail_vi0001_stock_macro_accessors", code, &fields);
    let mut sorted = positions.clone();
    sorted.sort();
    assert_eq!(positions, sorted, "arrow_* accessors are not declared in source order");
}

// An impl that does not have the generated shape keeps its written order: the
// verifier only reorders what stock's macro could have emitted in another order.
#[test]
fn sail_vi0001_other_impls_keep_written_order() {
    let code = verus_code_str! {
        pub enum F { A { ga: u8 }, B { gb: u8 } }
        impl F {
            pub open spec fn arrow_zb(self) -> int { 2 }
            pub open spec fn arrow_za(self) -> int { 1 }
        }
        proof fn uses(f: F) { assert(f.arrow_za() + f.arrow_zb() == 3); }
    }
    .to_string();
    let positions = arrow_positions("sail_vi0001_other_impls", code, &["zb", "za"]);
    assert!(positions[0] < positions[1], "a hand-written impl was reordered");
}
