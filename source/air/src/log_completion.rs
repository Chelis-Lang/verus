//! Producer-owned, log-only completion witnesses. These bytes never enter SMT input.

use std::io::{self, Write};

#[derive(Clone, Copy)]
pub(crate) enum Mode {
    ZeroProcess,
    ProcessUsed,
}

fn write_completion(writer: &mut dyn Write, kind: &str, mode: Mode) -> io::Result<()> {
    let mode = match mode {
        Mode::ZeroProcess => "zero-process",
        Mode::ProcessUsed => "process-used",
    };
    writeln!(writer, ";; SAIL-COMPLETE 1 {kind} {mode}")?;
    writer.flush()
}

pub(crate) fn write_air_completion(writer: &mut dyn Write, mode: Mode) -> io::Result<()> {
    write_completion(writer, "air", mode)
}

pub(crate) fn write_zero_transcript_completion(writer: &mut dyn Write) -> io::Result<()> {
    write_completion(writer, "transcript", Mode::ZeroProcess)
}

pub(crate) fn write_used_transcript_completion(
    writer: &mut dyn Write,
    commands_complete: bool,
) -> io::Result<()> {
    if !commands_complete {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cannot complete transcript with an outstanding solver response",
        ));
    }
    write_completion(writer, "transcript", Mode::ProcessUsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{Context, SmtSolver};
    use crate::emitter::Emitter;
    use crate::messages::AirMessageInterface;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    #[derive(Default)]
    struct State {
        bytes: Vec<u8>,
        writes: usize,
        flushes: usize,
    }

    struct Writer {
        state: Rc<RefCell<State>>,
        fail_write: bool,
        fail_flush: bool,
    }

    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let mut state = self.state.borrow_mut();
            state.writes += 1;
            if self.fail_write {
                return Err(io::Error::other("injected write failure"));
            }
            state.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.state.borrow_mut().flushes += 1;
            if self.fail_flush {
                return Err(io::Error::other("injected flush failure"));
            }
            Ok(())
        }
    }

    fn writer(state: &Rc<RefCell<State>>, fail_write: bool, fail_flush: bool) -> Box<Writer> {
        Box::new(Writer { state: state.clone(), fail_write, fail_flush })
    }

    fn context() -> Context {
        Context::new(Arc::new(AirMessageInterface {}), SmtSolver::Z3)
    }

    #[test]
    fn zero_process_pair_has_explicit_matching_completion() {
        let air = Rc::new(RefCell::new(State::default()));
        let transcript = Rc::new(RefCell::new(State::default()));
        let mut context = context();
        context.set_air_initial_log(writer(&air, false, false));
        context.set_smt_transcript_log(writer(&transcript, false, false));
        context.complete_transcript_logs().unwrap();
        assert_eq!(air.borrow().bytes, b";; SAIL-COMPLETE 1 air zero-process\n");
        assert_eq!(transcript.borrow().bytes, b";; SAIL-COMPLETE 1 transcript zero-process\n");
        assert_eq!(air.borrow().flushes, 1);
        assert_eq!(transcript.borrow().flushes, 1);
    }

    #[test]
    fn disabled_or_unpaired_logging_has_no_footer_io() {
        context().complete_transcript_logs().unwrap();
        for air_only in [false, true] {
            let state = Rc::new(RefCell::new(State::default()));
            let mut context = context();
            // Even a writer that rejects all I/O must remain untouched.
            if air_only {
                context.set_air_initial_log(writer(&state, true, true));
            } else {
                context.set_smt_transcript_log(writer(&state, true, true));
            }
            context.complete_transcript_logs().unwrap();
            assert_eq!(state.borrow().writes, 0);
            assert_eq!(state.borrow().flushes, 0);
        }
    }

    #[test]
    fn dropping_an_incomplete_context_writes_no_completion() {
        let state = Rc::new(RefCell::new(State::default()));
        let mut context = context();
        context.set_air_initial_log(writer(&state, false, false));
        context.set_smt_transcript_log(writer(&state, false, false));
        drop(context);
        assert_eq!(state.borrow().writes, 0);
        assert_eq!(state.borrow().flushes, 0);
    }

    #[test]
    fn pair_write_and_flush_errors_are_returned() {
        for fail_air in [false, true] {
            for fail_flush in [false, true] {
                let air = Rc::new(RefCell::new(State::default()));
                let transcript = Rc::new(RefCell::new(State::default()));
                let mut context = context();
                context.set_air_initial_log(writer(
                    &air,
                    fail_air && !fail_flush,
                    fail_air && fail_flush,
                ));
                context.set_smt_transcript_log(writer(
                    &transcript,
                    !fail_air && !fail_flush,
                    !fail_air && fail_flush,
                ));
                assert!(context.complete_transcript_logs().is_err());
            }
        }
    }

    #[test]
    fn pending_response_refuses_before_any_log_io() {
        let state = Rc::new(RefCell::new(State::default()));
        let mut writer = writer(&state, false, false);
        assert!(write_used_transcript_completion(writer.as_mut(), false).is_err());
        assert_eq!(state.borrow().writes, 0);
        assert_eq!(state.borrow().flushes, 0);
        write_used_transcript_completion(writer.as_mut(), true).unwrap();
        assert_eq!(state.borrow().bytes, b";; SAIL-COMPLETE 1 transcript process-used\n");
    }

    #[test]
    fn used_transcript_write_and_flush_errors_are_returned() {
        for fail_flush in [false, true] {
            let state = Rc::new(RefCell::new(State::default()));
            let mut writer = writer(&state, !fail_flush, fail_flush);
            assert!(write_used_transcript_completion(writer.as_mut(), true).is_err());
        }
    }

    #[test]
    fn emitter_footer_preserves_pending_solver_bytes() {
        let state = Rc::new(RefCell::new(State::default()));
        let interface = Arc::new(AirMessageInterface {});
        let mut logged = Emitter::new(
            interface.clone(),
            true,
            true,
            Some(writer(&state, false, false)),
            SmtSolver::Z3,
        );
        let mut unlogged = Emitter::new(interface, true, true, None, SmtSolver::Z3);
        logged.log_set_option("rlimit", "100");
        unlogged.log_set_option("rlimit", "100");
        logged.complete_log(Mode::ProcessUsed).unwrap();
        unlogged.complete_log(Mode::ProcessUsed).unwrap();
        assert_eq!(logged.take_pipe_data(), unlogged.take_pipe_data());
        assert!(state.borrow().bytes.ends_with(b";; SAIL-COMPLETE 1 air process-used\n"));
    }
}
