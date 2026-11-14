//! Opt-in compiler-owned mutation admission facts. These observations never
//! choose a verifier result or change the AST, solver input or diagnostics.
//! Consumers require a complete inventory, exact source checksums and their
//! own Cargo/runtime/input binding before admitting any source edit.

use super::*;
use rustc_span::source_map::SourceMap;
use serde::Serialize;

const MAX_FUNCTIONS: usize = 100_000;
const MAX_NAME: usize = 4096;

#[derive(Serialize)]
struct SourceSpan {
    file: String,
    checksum_algorithm: String,
    checksum: String,
    source_bytes: u32,
    start: u32,
    end: u32,
}

fn source_span(map: &SourceMap, span: &vir::messages::Span) -> Option<SourceSpan> {
    // A local raw span belongs to rustc's existing source map. Do not import
    // source files or reopen a path to pretend it supplied earlier bytes.
    let span = crate::spans::from_raw_span(&span.raw_span)?;
    if span.is_dummy() {
        return None;
    }
    let file = map.lookup_source_file(span.lo());
    if !file.is_real_file() || file.is_imported() || span.hi() > file.end_position() {
        return None;
    }
    let name = file.name.prefer_local_unconditionally().to_string();
    if name.len() > MAX_NAME {
        return None;
    }
    // The mutator edits original UTF-8 bytes. rustc positions refer to its
    // normalized source; this conversion accounts for BOM and CRLF removal.
    let start = file.original_relative_byte_pos(span.lo()).0;
    let end = file.original_relative_byte_pos(span.hi()).0;
    if start > end || end > file.unnormalized_source_len {
        return None;
    }
    let checksum = file.checksum_hash?;
    Some(SourceSpan {
        file: name,
        checksum_algorithm: format!("{:?}", checksum.kind),
        checksum: checksum.to_string(),
        source_bytes: file.unnormalized_source_len,
        start,
        end,
    })
}

pub(super) fn emit(verifier: &Verifier, compiler: &Compiler, krate: &Krate, global: &GlobalCtx) {
    if !sail_engine::campaign_facts_active() {
        return;
    }
    let Some(crate_id) = verifier.crate_id.as_ref() else {
        return;
    };
    let count = krate.functions.iter().filter(|f| &f.x.name.path.krate == crate_id).count();
    if count > MAX_FUNCTIONS {
        sail_engine::emit!("campaign-facts-unavailable", {"reason": "function inventory bound"});
        return;
    }
    let map = compiler.sess.source_map();
    for (ordinal, function) in
        krate.functions.iter().filter(|f| &f.x.name.path.krate == crate_id).enumerate()
    {
        let name = sail_engine::ObservedFriendly(&function.x.name);
        let unique = sail_engine::unique(verifier, &function.x.name);
        let source = source_span(map, &function.span);
        let body = function.x.body.as_ref().and_then(|body| source_span(map, &body.span));
        let node = vir::recursion::Node::Fun(function.x.name.clone());
        let recursive = global
            .func_call_graph
            .contains_node(&node)
            .then(|| global.func_call_graph.node_is_in_cycle(&node));
        sail_engine::emit!("campaign-function", {
            "schema": 2, "ordinal": ordinal, "function": name,
            "function_path": crate::util::sail_function_path(&function.x.name),
            "unique": unique,
            "source": source, "body": body, "mode": function.x.mode,
            "item_kind": function.x.item_kind, "recursive": recursive,
            "when_used_as_spec": function.x.attrs.autospec.is_some()
        });
    }
    // This marker attests inventory coverage only. The owner must additionally
    // require all ordinals, actual compiler completion, full input binding and
    // absence of unavailable/malformed/duplicate facts. Missing evidence refuses.
    sail_engine::emit!("campaign-facts-complete", {
        "schema": 2, "functions": count, "graph": "verifier-crate-before-bucket-pruning"
    });
}

/// Records identity at the actual query-result producer, before messages may
/// be buffered, deduplicated or rendered on a different reporting thread.
/// The consumer joins this original structured message to the producing
/// mode's diagnostics; source-line containment confers no attribution.
pub(super) fn diagnostic(
    verifier: &Verifier,
    function: &Fun,
    message: &ArcDynMessage,
    level: MessageLevel,
    failure: &str,
) {
    if !sail_engine::campaign_requested() {
        return;
    }
    let name = sail_engine::ObservedFriendly(function);
    let unique = sail_engine::unique(verifier, function);
    if let Some(message) = message.downcast_ref::<MessageX>() {
        sail_engine::emit!("campaign-diagnostic", {
            "schema": 1, "function": name, "unique": unique,
            "message": message, "level": level, "failure": failure
        });
    } else {
        sail_engine::emit!("campaign-diagnostic-unavailable", {
            "schema": 1, "function": name, "reason": "unsupported message owner"
        });
    }
}
