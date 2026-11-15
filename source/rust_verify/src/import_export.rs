use crate::config::Args;
use crate::spans::FileStartEndPos;
use crate::verifier::io_vir_err;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::sync::Arc;
use vir::ast::{Krate, Mode, VirErr};

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CrateMetadata {
    pub crate_id: u64,
    #[serde(serialize_with = "serialize_original_files")]
    pub original_files: HashMap<Vec<u8>, FileStartEndPos>,
}

// Keep lookup and decoding unchanged, but make the on-disk map independent of
// RandomState and insertion order. Bincode uses the same map representation for
// BTreeMap and HashMap; only the order of complete key/value entries changes.
fn serialize_original_files<S: Serializer>(
    files: &HashMap<Vec<u8>, FileStartEndPos>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    files.iter().collect::<BTreeMap<_, _>>().serialize(serializer)
}

#[cfg(test)]
mod metadata_order_tests {
    use super::*;

    fn position(start: u32) -> FileStartEndPos {
        serde_json::from_value(serde_json::json!({
            "filename": "part.rs", "start_pos": start, "end_pos": start + 10
        }))
        .unwrap()
    }

    #[test]
    fn original_files_serialize_in_key_order_and_preserve_values() {
        // In this pinned bincode configuration, maps and sequences of pairs
        // both encode a length followed by key/value entries. Inspect their
        // actual wire order without putting them back into a HashMap.
        #[derive(Deserialize, Serialize)]
        struct WireMetadata {
            crate_id: u64,
            original_files: Vec<(Vec<u8>, FileStartEndPos)>,
        }
        let config = bincode_next::config::legacy();
        let mut expected = None;
        for order in [[2, 0, 1], [1, 2, 0], [0, 1, 2], [2, 1, 0], [0, 2, 1], [1, 0, 2]] {
            for _ in 0..8 {
                let files = order.into_iter().map(|i| (vec![i], position(u32::from(i)))).collect();
                let metadata = CrateMetadata { crate_id: 42, original_files: files };
                let bytes = bincode_next::serde::encode_to_vec(&metadata, config).unwrap();
                if let Some(expected) = &expected {
                    assert_eq!(&bytes, expected);
                } else {
                    expected = Some(bytes.clone());
                }
                let (wire, consumed): (WireMetadata, _) =
                    bincode_next::serde::decode_from_slice(&bytes, config).unwrap();
                assert_eq!(consumed, bytes.len());
                assert_eq!(
                    wire.original_files.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>(),
                    vec![vec![0], vec![1], vec![2]]
                );
                let (decoded, consumed): (CrateMetadata, _) =
                    bincode_next::serde::decode_from_slice(&bytes, config).unwrap();
                assert_eq!(consumed, bytes.len());
                assert_eq!(decoded.crate_id, 42);
                assert_eq!(decoded.original_files.len(), 3);
                for i in 0..3u8 {
                    assert_eq!(
                        serde_json::to_value(&decoded.original_files[&vec![i]]).unwrap(),
                        serde_json::to_value(position(u32::from(i))).unwrap()
                    );
                }
                let mut changed = metadata;
                changed.original_files.insert(vec![1], position(99));
                assert_ne!(bincode_next::serde::encode_to_vec(&changed, config).unwrap(), bytes);
            }
        }
        // Legacy unsorted map entries remain readable; no new wire format or
        // decoder normalization is introduced by the serializer correction.
        let old = WireMetadata {
            crate_id: 43,
            original_files: vec![(vec![2], position(9)), (vec![1], position(7))],
        };
        let old_bytes = bincode_next::serde::encode_to_vec(&old, config).unwrap();
        let (decoded, consumed): (CrateMetadata, _) =
            bincode_next::serde::decode_from_slice(&old_bytes, config).unwrap();
        assert_eq!(consumed, old_bytes.len());
        assert_eq!(decoded.crate_id, 43);
        assert_eq!(decoded.original_files.len(), 2);
        for (key, start) in [(1, 7), (2, 9)] {
            assert_eq!(
                serde_json::to_value(&decoded.original_files[&vec![key]]).unwrap(),
                serde_json::to_value(position(start)).unwrap()
            );
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CrateWithMetadata {
    krate: Krate,
    metadata: CrateMetadata,
}

pub(crate) struct ImportOutput {
    pub(crate) vir_crates: Vec<Krate>,
    pub(crate) metadatas: Vec<CrateMetadata>,
}

pub(crate) fn import_crates(
    args: &Args,
    import_virs_via_cargo: Vec<(String, String)>,
) -> Result<ImportOutput, VirErr> {
    let mut metadatas = Vec::new();
    let mut vir_crates = Vec::new();
    for (name, file_path) in args.import.iter().chain(import_virs_via_cargo.iter()) {
        let file = match std::fs::File::open(file_path) {
            Ok(file) => file,
            Err(err) => {
                return Err(io_vir_err(
                    format!("could not open imported library file `{file_path}`"),
                    err,
                ));
            }
        };
        let mut file = std::io::BufReader::new(crate::sail_campaign_io::Reader::new(file));
        // Sail: optionally rename recorded span strings (`sail_vir_span_remap`).
        let decoded =
            vir::messages::with_imported_span_remap(crate::sail_vir_span_remap::remap(), || {
                bincode_next::serde::decode_from_std_read(&mut file, bincode_next::config::legacy())
            });
        let CrateWithMetadata { krate, metadata } = match decoded {
            Ok(crate_with_metadata) => crate_with_metadata,
            Err(_e) => {
                return Err(crate::util::error(format!(
                    "failed to deserialize imported library file `{file_path}` - it may need to be rebuilt by Verus"
                )));
            }
        };
        file.get_ref().retain(name, file_path);
        //   let libcrate: Krate = serde_json::from_reader(file).expect("read crate from file");
        // We can also look at other packages: https://github.com/djkoloski/rust_serialization_benchmark
        vir_crates.push(krate);
        metadatas.push(metadata);
    }
    Ok(ImportOutput { vir_crates, metadatas })
}

pub(crate) fn export_crate(
    args: &Args,
    export_vir_path_via_cargo: &Option<std::path::PathBuf>,
    vir_metadata: CrateMetadata,
    vir_crate: Krate,
) -> Result<(), VirErr> {
    let export = args.export.as_ref().map(|s| s.clone().into());
    if let Some(file_path) = export.as_ref().or(export_vir_path_via_cargo.as_ref()) {
        // for efficiency's sake, prune out elements of AST that won't be needed by importers:
        let mut kratex = (*vir_crate).clone();
        kratex.functions.retain(|f| f.x.visibility.restricted_to.is_none());
        for func in kratex.functions.iter_mut() {
            let mut functionx = func.x.clone();
            functionx.decrease_by = None;
            if (functionx.mode != Mode::Spec || !functionx.body_visibility.is_public())
                && !matches!(&functionx.kind, vir::ast::FunctionKind::TraitMethodDecl { .. })
            {
                functionx.body = None;
            }
            *func = func.new_x(functionx);
        }
        let vir_crate = Arc::new(kratex);

        let file = match std::fs::File::create(file_path) {
            Ok(file) => file,
            Err(err) => {
                return Err(io_vir_err(
                    format!("could not create exported library file `{:?}`", file_path),
                    err,
                ));
            }
        };
        let mut file = std::io::BufWriter::new(crate::sail_campaign_io::Writer::new(file));
        let krate_with_metadata = CrateWithMetadata { krate: vir_crate, metadata: vir_metadata };
        bincode_next::serde::encode_into_std_write(
            &krate_with_metadata,
            &mut file,
            bincode_next::config::legacy(),
        )
        .expect("write crate to file");
        if crate::sail_compiler_inputs::campaign_requested() {
            let flushed = file.flush().is_ok();
            file.get_ref().retain(file_path, flushed);
        }
        //serde_json::to_writer(file, &vir_crate).expect("write crate to file");
        //serde_json::to_writer_pretty(file, &vir_crate).expect("write crate to file");
    }
    Ok(())
}
