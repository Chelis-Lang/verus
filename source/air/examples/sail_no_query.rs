//! Untimed evidence validator using this source's genuine AIR grammar.
//! No Context or solver is constructed. This is not verification.

use air::ast::CommandX;
use air::messages::AirMessageInterface;
use std::io::Read;
use std::sync::Arc;

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 256;

fn validate(bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("AIR input exceeds the limit");
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "AIR is not UTF8")?;
    // The inserted newline keeps a final comment from consuming the closing
    // wrapper. The wrapper permits the genuine parser to consume all forms.
    let wrapped = format!("(\n{text}\n)");
    let mut lexical = sise::Parser::new(&wrapped);
    let mut depth = 0usize;
    loop {
        match lexical.next_item().map_err(|_| "invalid AIR syntax")? {
            sise::ParsedItem::ListStart(_) => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err("AIR nesting exceeds the limit");
                }
            }
            sise::ParsedItem::ListEnd(_) => {
                depth = depth.checked_sub(1).ok_or("invalid AIR nesting")?;
                if depth == 0 {
                    break;
                }
            }
            sise::ParsedItem::Atom(..) => {}
        }
    }
    lexical.finish().map_err(|_| "trailing AIR input")?;
    let mut parser = sise::Parser::new(&wrapped);
    let tree = sise::parse_tree(&mut parser).map_err(|_| "invalid AIR syntax")?;
    parser.finish().map_err(|_| "trailing AIR input")?;
    let sise::TreeNode::List(nodes) = tree else {
        return Err("expected AIR command list");
    };
    // The original AIR grammar may panic on malformed nested nodes. Such a
    // parser failure is a refusal, never a successful no-query observation.
    let commands = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        air::parser::Parser::new(Arc::new(AirMessageInterface {})).nodes_to_commands(&nodes)
    }))
    .map_err(|_| "AIR grammar failed")?
    .map_err(|_| "invalid AIR command")?;
    let mut scopes = 0usize;
    for command in commands.iter() {
        match command.as_ref() {
            CommandX::Push => scopes += 1,
            CommandX::Pop => scopes = scopes.checked_sub(1).ok_or("AIR scope underflow")?,
            CommandX::SetOption(..) | CommandX::Global(..) => {}
            _ => return Err("AIR contains a query command"),
        }
    }
    if scopes != 0 {
        return Err("AIR has an unfinished scope");
    }
    Ok(())
}

fn run() -> Result<(), &'static str> {
    if std::env::args_os().len() != 1 {
        return Err("this validator takes AIR bytes on stdin and no arguments");
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read AIR input")?;
    validate(&bytes)
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => {
            println!("SAIL-AIR-NO-QUERY 1");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_recognized_commands_without_queries() {
        assert!(validate(b";; a comment\n(set-option :auto_config false)\n(declare-sort T 0)\n(push)\n(declare-const x T)\n(axiom true)\n(pop)\n").is_ok());
    }

    #[test]
    fn query_forms_and_unknown_or_malformed_grammar_refuse() {
        for text in [
            "(check-valid (assert true))",
            " \n (check-valid (assert true))",
            "(|check-valid| (assert true))",
            "(unknown-command)",
            "(declare-sort T 1)",
            "(declare-fun f not-a-list Bool)",
            "(declare-const x ())",
            "(axiom ())",
            "(declare-sort T 0",
            "(declare-sort T 0)) (declare-sort U 0)",
            "(pop)",
            "(push)",
            "(push)(pop)(pop)",
            "\"unfinished",
            "unexpected-atom",
        ] {
            assert!(validate(text.as_bytes()).is_err(), "accepted {text:?}");
        }
        assert!(validate(&[0xff]).is_err());
    }

    #[test]
    fn byte_and_nesting_limits_refuse() {
        assert!(validate(&vec![b' '; MAX_BYTES as usize + 1]).is_err());
        assert!(validate("(".repeat(MAX_DEPTH + 1).as_bytes()).is_err());
    }
}
