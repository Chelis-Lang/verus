//! Keep paired observation filenames within a portable component limit.

use crate::config;
use sha2::{Digest, Sha256};

const MAX_COMPONENT_BYTES: usize = 255;
const READABLE_PREFIX_BYTES: usize = 120;

// Compound suffixes precede their terminal extension. Members of a family
// share both a shortening decision and the digest of the complete stem, so
// e.g. transcript.with_suffix(".air") still names the corresponding AIR file.
const QUERY_SUFFIXES: &[&str] = &[
    config::AIR_FINAL_FILE_SUFFIX,
    config::AIR_INITIAL_FILE_SUFFIX,
    config::SMT_FILE_SUFFIX,
    config::SMT_TRANSCRIPT_FILE_SUFFIX,
    config::PROFILE_FILE_SUFFIX,
    config::SINGULAR_FILE_SUFFIX,
];
const SUFFIX_FAMILIES: &[&[&str]] = &[
    QUERY_SUFFIXES,
    &[
        config::VIR_SIMPLE_FILE_SUFFIX,
        config::VIR_POLY_FILE_SUFFIX,
        config::VIR_SST_FILE_SUFFIX,
        config::VIR_FILE_SUFFIX,
    ],
    &[
        config::CALL_GRAPH_FILE_SUFFIX_FULL_INITIAL,
        config::CALL_GRAPH_FILE_SUFFIX_FULL_SIMPLIFIED,
        config::CALL_GRAPH_FILE_SUFFIX_NOSTD_INITIAL,
        config::CALL_GRAPH_FILE_SUFFIX_NOSTD_SIMPLIFIED,
    ],
    &[config::TRAIT_CONFLICT_FILE_SUFFIX],
    &[config::INTERPRETER_FILE_SUFFIX],
    &[config::TRIGGERS_FILE_SUFFIX],
    &[config::IMPL_NAMES_SUFFIX],
];

fn split_suffix(name: &str) -> (&str, &str, usize) {
    for family in SUFFIX_FAMILIES {
        for &suffix in *family {
            if let Some(stem) = name.strip_suffix(suffix) {
                let width = family.iter().map(|suffix| suffix.len()).max().unwrap_or(0);
                return (stem, suffix, width);
            }
        }
    }
    (name, "", 0)
}

// Preserve Sail's trailing spinoff grammar after shortening. The full original
// stem (including the original counter spelling) still enters the digest.
fn spinoff_tail(stem: &str) -> String {
    let (base, expansion) = stem.strip_suffix("_expand").map_or((stem, ""), |s| (s, "_expand"));
    let Some((_, digits)) = base.rsplit_once("._") else { return String::new() };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return String::new();
    }
    let Ok(counter) = digits.parse::<u32>() else { return String::new() };
    // Generated counters use at least two digits. Canonical decimal also bounds
    // adversarial leading-zero input without changing its parsed spinoff index.
    format!("._{counter:02}{expansion}")
}

pub(super) fn bounded(name: String) -> String {
    let (stem, suffix, family_width) = split_suffix(&name);
    // A short .air name must also shorten when its paired transcript would
    // exceed the limit, even if this particular component fits by itself.
    if stem.len() + family_width <= MAX_COMPONENT_BYTES {
        return name;
    }
    let tail = spinoff_tail(stem);
    let budget = MAX_COMPONENT_BYTES - family_width - 1 - 64 - tail.len();
    let mut end = READABLE_PREFIX_BYTES.min(budget).min(stem.len());
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    // Bind all function/rerun/counter bytes. Kind stays outside this digest:
    // paired outputs need the same stem but their complete distinct suffixes.
    let digest = hex::encode(Sha256::digest(stem.as_bytes()));
    format!("{}-{digest}{tail}{suffix}", &stem[..end])
}

#[cfg(test)]
mod tests {
    use super::{QUERY_SUFFIXES, SUFFIX_FAMILIES, bounded};
    use std::path::Path;

    #[test]
    fn short_complete_families_remain_byte_exact() {
        for family in SUFFIX_FAMILIES {
            let width = family.iter().map(|suffix| suffix.len()).max().unwrap();
            let stem = "x".repeat(255 - width);
            for suffix in *family {
                let name = format!("{stem}{suffix}");
                assert_eq!(bounded(name.clone()), name);
            }
        }
        assert_eq!(bounded("crate.vir".to_string()), "crate.vir");
    }

    #[test]
    fn actual_anvil_spinoff_components_keep_a_paired_stem() {
        let module = "vdeployment_controller__proof__liveness__resource_match";
        let function =
            "lemma_from_after_ensure_new_vrs_with_old_vrs_of_n_to_pending_scale_down_req_in_flight";
        let path = "verifiable_controllers!vdeployment_controller.proof.liveness.resource_match";
        let stem = format!("{module}__{function}{path}.{function}._01");
        assert_eq!(format!("{stem}.air").len(), 311);
        let air = bounded(format!("{stem}.air"));
        let transcript = bounded(format!("{stem}.smt_transcript"));
        assert_eq!(Path::new(&transcript).with_extension("air"), Path::new(&air));
        assert_eq!(air, bounded(format!("{stem}.air")));
        let shortened_stem = air.strip_suffix(".air").unwrap();
        for suffix in QUERY_SUFFIXES {
            let actual = bounded(format!("{stem}{suffix}"));
            assert!(actual.len() <= 255);
            assert_eq!(actual, format!("{shortened_stem}{suffix}"));
        }
    }

    #[test]
    fn paired_boundary_crossing_shortens_even_the_air_member() {
        let stem = "x".repeat(255 - ".air".len());
        let old_air = format!("{stem}.air");
        assert_eq!(old_air.len(), 255);
        assert!(format!("{stem}.smt_transcript").len() > 255);
        let air = bounded(old_air.clone());
        let transcript = bounded(format!("{stem}.smt_transcript"));
        assert_ne!(air, old_air);
        assert_eq!(Path::new(&transcript).with_extension("air"), Path::new(&air));
        assert!(transcript.len() <= 255);
    }

    #[test]
    fn complete_stem_identity_includes_function_and_counter() {
        let prefix = "x".repeat(300);
        let first = bounded(format!("{prefix}.first._01.smt_transcript"));
        let second = bounded(format!("{prefix}.second._01.smt_transcript"));
        let next = bounded(format!("{prefix}.first._02.smt_transcript"));
        assert_ne!(first, second);
        assert_ne!(first, next);
    }

    #[test]
    fn shortened_spinoffs_keep_counter_and_expansion_grammar() {
        let prefix = "x".repeat(400);
        for counter in ["01", "12", "4294967295"] {
            for expand in ["", "_expand"] {
                let stem = format!("{prefix}._{counter}{expand}");
                let air = bounded(format!("{stem}.air"));
                let transcript = bounded(format!("{stem}.smt_transcript"));
                assert!(air.ends_with(&format!("._{counter}{expand}.air")));
                assert!(transcript.ends_with(&format!("._{counter}{expand}.smt_transcript")));
                assert_eq!(Path::new(&transcript).with_extension("air"), Path::new(&air));
                assert!(transcript.len() <= 255);
            }
        }
        let padded = bounded(format!("{prefix}._{}1_expand.smt_transcript", "0".repeat(400)));
        assert!(padded.ends_with("._01_expand.smt_transcript"));
        assert!(padded.len() <= 255);
        let malformed = bounded(format!("{prefix}._ab.smt_transcript"));
        assert!(!malformed.ends_with("._ab.smt_transcript"));
    }

    #[test]
    fn unicode_and_unknown_names_remain_bounded() {
        let unicode = bounded("x".repeat(119) + &"中".repeat(100) + ".profile");
        assert!(unicode.len() <= 255);
        assert!(unicode.ends_with(".profile"));
        let unknown = bounded("x".repeat(300) + "." + &"y".repeat(300));
        assert!(unknown.len() <= 255);
    }
}
