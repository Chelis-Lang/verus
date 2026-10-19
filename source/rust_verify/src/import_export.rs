use crate::config::Args;
use crate::spans::FileStartEndPos;
use crate::verifier::io_vir_err;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use vir::ast::{Krate, Mode, VirErr};

// ClosureId and concrete trait naming must never be decoded as an old export.
// Bump this fixed wire-policy version whenever identity/serialization changes.
const EXPORT_FORMAT: &[u8] = b"VERUS-SAIL-IMPL-ID-02\0";

fn read_export_header(reader: &mut impl Read) -> std::io::Result<()> {
    let mut actual = [0u8; EXPORT_FORMAT.len()];
    reader.read_exact(&mut actual)?;
    if actual.as_slice() != EXPORT_FORMAT {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "export format mismatch"));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CrateMetadata {
    pub crate_id: u64,
    pub original_files: HashMap<Vec<u8>, FileStartEndPos>,
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
    for (_, file_path) in args.import.iter().chain(import_virs_via_cargo.iter()) {
        let mut file = std::io::BufReader::new(match std::fs::File::open(file_path) {
            Ok(file) => file,
            Err(err) => {
                return Err(io_vir_err(
                    format!("could not open imported library file `{file_path}`"),
                    err,
                ));
            }
        });
        if read_export_header(&mut file).is_err() {
            return Err(crate::util::error(format!(
                "incompatible or truncated imported library file `{file_path}` - rebuild vstd and all exports with this Verus runtime"
            )));
        }
        let CrateWithMetadata { krate, metadata } = match bincode_next::serde::decode_from_std_read(
            &mut file,
            bincode_next::config::legacy(),
        ) {
            Ok(crate_with_metadata) => crate_with_metadata,
            Err(_e) => {
                return Err(crate::util::error(format!(
                    "failed to deserialize imported library file `{file_path}` - it may need to be rebuilt by Verus"
                )));
            }
        };
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

        let mut file = std::io::BufWriter::new(match std::fs::File::create(file_path) {
            Ok(file) => file,
            Err(err) => {
                return Err(io_vir_err(
                    format!("could not create exported library file `{:?}`", file_path),
                    err,
                ));
            }
        });
        file.write_all(EXPORT_FORMAT).map_err(|err| {
            io_vir_err(format!("could not write exported library header `{:?}`", file_path), err)
        })?;
        let krate_with_metadata = CrateWithMetadata { krate: vir_crate, metadata: vir_metadata };
        bincode_next::serde::encode_into_std_write(
            &krate_with_metadata,
            &mut file,
            bincode_next::config::legacy(),
        )
        .expect("write crate to file");
        //serde_json::to_writer(file, &vir_crate).expect("write crate to file");
        //serde_json::to_writer_pretty(file, &vir_crate).expect("write crate to file");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_export() -> CrateWithMetadata {
        CrateWithMetadata {
            krate: Arc::new(vir::ast::KrateX {
                functions: vec![],
                reveal_groups: vec![],
                datatypes: vec![],
                traits: vec![],
                trait_impls: vec![],
                assoc_type_impls: vec![],
                modules: vec![],
                external_fns: vec![],
                external_types: vec![],
                path_as_rust_names: vec![],
                opaque_types: vec![],
                arch: vir::ast::Arch { word_bits: vir::ast::ArchWordBits::Either32Or64 },
            }),
            metadata: CrateMetadata { crate_id: 17, original_files: HashMap::new() },
        }
    }

    #[test]
    fn old_missing_mismatched_and_truncated_exports_refuse_before_decode() {
        // A genuine old-format complete payload still decodes under the old
        // bincode schema, but cannot pass the new format boundary.
        let old =
            bincode_next::serde::encode_to_vec(empty_export(), bincode_next::config::legacy())
                .unwrap();
        let (decoded, _): (CrateWithMetadata, _) =
            bincode_next::serde::decode_from_slice(&old, bincode_next::config::legacy()).unwrap();
        assert_eq!(decoded.metadata.crate_id, 17);
        assert!(read_export_header(&mut &old[..]).is_err());
        for length in 0..EXPORT_FORMAT.len() {
            assert!(read_export_header(&mut &EXPORT_FORMAT[..length]).is_err());
        }
        let mut wrong = EXPORT_FORMAT.to_vec();
        wrong[0] ^= 1;
        assert!(read_export_header(&mut &wrong[..]).is_err());
    }

    #[test]
    fn matching_format_round_trips_actual_crate_metadata() {
        let mut bytes = EXPORT_FORMAT.to_vec();
        bincode_next::serde::encode_into_std_write(
            empty_export(),
            &mut bytes,
            bincode_next::config::legacy(),
        )
        .unwrap();
        let mut reader = &bytes[..];
        read_export_header(&mut reader).unwrap();
        let decoded: CrateWithMetadata =
            bincode_next::serde::decode_from_std_read(&mut reader, bincode_next::config::legacy())
                .unwrap();
        assert_eq!(decoded.metadata.crate_id, 17);
        assert!(decoded.krate.functions.is_empty());
        assert!(reader.is_empty());
    }
}
