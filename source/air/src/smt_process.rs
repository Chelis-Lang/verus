use yansi::Paint;

use crate::context::SmtSolver;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout};
use std::sync::mpsc::{Receiver, Sender, channel};

struct SolverInfo {
    executable_name: &'static str,
    env_path_var: &'static str,
}

impl SolverInfo {
    pub fn new(solver: &SmtSolver) -> Self {
        match solver {
            SmtSolver::Z3 => SolverInfo { executable_name: "z3", env_path_var: "VERUS_Z3_PATH" },
            SmtSolver::Cvc5 => {
                SolverInfo { executable_name: "cvc5", env_path_var: "VERUS_CVC5_PATH" }
            }
        }
    }

    pub fn executable(&self) -> String {
        if let Ok(path) = std::env::var(self.env_path_var) {
            path
        } else if cfg!(windows) {
            self.executable_name.to_string() + ".exe"
        } else {
            self.executable_name.to_string()
        }
    }
}

pub struct SmtProcess {
    requests: Option<Sender<Vec<u8>>>,
    responses_buf_recv:
        Option<(BufReader<ChildStdout>, Receiver<(BufReader<ChildStdout>, Vec<String>)>)>,
    recv_requests: Sender<BufReader<ChildStdout>>,
    child: Child,
    transcript_log: Option<Box<dyn std::io::Write>>,
}

const DONE: &str = "<<DONE>>";
const DONE_QUOTED: &str = "\"<<DONE>>\"";

/// A separate thread writes data to the SMT solver over a pipe.
/// (Rust's documentation says you need a separate thread; otherwise, it lets the pipes deadlock.)
pub(crate) fn writer_thread(requests: Receiver<Vec<u8>>, mut smt_pipe_stdin: ChildStdin) {
    while let Ok(req) = requests.recv() {
        smt_pipe_stdin
            .write_all(&req)
            .and_then(|_| writeln!(&smt_pipe_stdin))
            // Ask the solver to print DONE, so we know when it is done
            .and_then(|_| writeln!(&smt_pipe_stdin, "(echo \"{}\")", DONE))
            .and_then(|_| smt_pipe_stdin.flush())
            // The solver process could die unexpectedly.  In that case, we die too:
            .expect("IO error: failure when sending data to solver process across pipe");
    }
    // Exit when the other side closes the channel
}

/// A separate thread read data from the SMT solver over a pipe.
fn reader_thread(
    recv_requests: Receiver<BufReader<ChildStdout>>,
    responses: Sender<(BufReader<ChildStdout>, Vec<String>)>,
    solver: SmtSolver,
) {
    while let Ok(mut smt_pipe_stdout) = recv_requests.recv() {
        let mut lines = Vec::new();
        let mut empty_lines = 0;
        loop {
            let mut line = String::new();
            smt_pipe_stdout
                .read_line(&mut line)
                // The solver process could die unexpectedly.  In that case, we die too:
                .expect("IO error: failure when receiving data from solver process across pipe");
            line = line.replace("\n", "").replace("\r", "");
            if line == "" {
                empty_lines += 1;
            } else {
                empty_lines = 0;
                if line
                    == match solver {
                        SmtSolver::Z3 => DONE,
                        SmtSolver::Cvc5 => DONE_QUOTED,
                    }
                {
                    responses
                        .send((smt_pipe_stdout, lines))
                        .expect("internal error: solver reader thread failure");
                    break;
                }
            }
            if empty_lines > 2 {
                panic!("Got too many empty lines!");
            }
            lines.push(line);
        }
    }
}

impl SmtProcess {
    pub fn launch(solver: &SmtSolver, transcript_log: Option<Box<dyn std::io::Write>>) -> Self {
        let solver_info = SolverInfo::new(solver);
        let mut child = match std::process::Command::new(solver_info.executable())
            .args(match solver {
                SmtSolver::Z3 => vec!["-smt2", "-in"],
                SmtSolver::Cvc5 => vec![
                    "--no-interactive",    // We don't need a human interface
                    "--produce-models",    // Needed for error reporting
                    "--quant-dsplit=none", // Recommended by Andrew Reynolds (@ajreynol)
                    "--no-cbqi",           // Recommended by Andrew Reynolds (@ajreynol)
                    "--user-pat=strict",   // Recommended by Andrew Reynolds (@ajreynol)
                ],
            })
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                eprintln!(
                    "{}",
                    format!(
                        "error: could not execute {} process ({})",
                        solver_info.executable_name, err
                    )
                    .red()
                );
                eprintln!(
                    "help: {name} needs to be in the PATH, or the environment variable {var} must be set to the path of the {name} executable",
                    name = solver_info.executable_name,
                    var = solver_info.env_path_var
                );
                std::process::exit(128);
            }
        };
        let smt_pipe_stdout = BufReader::new(child.stdout.take().expect("take stdout"));
        let child_stdin = child.stdin.take().expect("take stdin");
        let (requests_sender, requests_receiver) = channel();
        let (responses_sender, responses_receiver) = channel();
        let (recv_responses_sender, recv_responses_receiver) = channel();
        let solver_clone = solver.clone();
        std::thread::spawn(move || writer_thread(requests_receiver, child_stdin));
        std::thread::spawn(move || {
            reader_thread(recv_responses_receiver, responses_sender, solver_clone)
        });
        SmtProcess {
            requests: Some(requests_sender),
            responses_buf_recv: Some((smt_pipe_stdout, responses_receiver)),
            recv_requests: recv_responses_sender,
            child: child,
            transcript_log,
        }
    }

    pub(crate) fn set_transcript_log(&mut self, writer: Box<dyn std::io::Write>) {
        self.transcript_log = Some(writer);
    }

    pub(crate) fn has_transcript_log(&self) -> bool {
        self.transcript_log.is_some()
    }

    /// Complete the log only after every CommandsHandle has returned its response.
    /// This does not send commands, start work, or wait for a response.
    pub(crate) fn complete_transcript_log(&mut self) -> std::io::Result<()> {
        if let Some(writer) = &mut self.transcript_log {
            crate::log_completion::write_used_transcript_completion(
                writer.as_mut(),
                self.responses_buf_recv.is_some(),
            )?;
        }
        Ok(())
    }

    /// Send commands to the solver, wait for acknowledgement, and return responses
    pub(crate) fn send_commands(&mut self, commands: Vec<u8>) -> Vec<String> {
        self.send_commands_async(commands).wait()
    }

    /// Send commands to the solver
    pub(crate) fn send_commands_async<'a>(&'a mut self, commands: Vec<u8>) -> CommandsHandle<'a> {
        // Send request to writer thread
        if let Some(writer) = &mut self.transcript_log {
            writeln!(writer, ";;;>>> QUERY").unwrap();
            writer.write_all(&commands).unwrap();
            writeln!(writer, ";;;<<<").unwrap();
            writer.flush().unwrap();
        }
        self.requests
            .as_mut()
            .unwrap()
            .send(commands)
            .expect("internal error: failed to send to writer thread");

        let (smt_pipe_stdout, receiver) = self
            .responses_buf_recv
            .take()
            .expect("internal error: wait on the CommandsHandle first");

        // Send read request to reader thread
        self.recv_requests
            .send(smt_pipe_stdout)
            .expect("internal error: failed to send to reader thread");

        CommandsHandle { smt_process: self, receiver }
    }
}

pub struct CommandsHandle<'a> {
    smt_process: &'a mut SmtProcess,
    receiver: std::sync::mpsc::Receiver<(BufReader<ChildStdout>, Vec<String>)>,
}

impl<'a> CommandsHandle<'a> {
    fn log_result(&mut self, result: &Vec<String>) {
        if let Some(writer) = &mut self.smt_process.transcript_log {
            writeln!(writer, ";;;>>> RESPONSE").unwrap();
            for line in result {
                writeln!(writer, "{}", line).unwrap();
            }
            writeln!(writer, ";;;<<<").unwrap();
            writer.flush().unwrap();
        }
    }

    pub fn wait(mut self) -> Vec<String> {
        let (smt_pipe_stdout, result) =
            self.receiver.recv().expect("internal error: solver reader thread failure");
        self.log_result(&result);
        self.smt_process.responses_buf_recv = Some((smt_pipe_stdout, self.receiver));
        result
    }

    pub fn wait_timeout(mut self, timeout: std::time::Duration) -> Result<Vec<String>, Self> {
        match self.receiver.recv_timeout(timeout) {
            Ok((smt_pipe_stdout, result)) => {
                self.log_result(&result);
                self.smt_process.responses_buf_recv = Some((smt_pipe_stdout, self.receiver));
                Ok(result)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(self),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("internal error: solver reader thread disconnected")
            }
        }
    }
}

impl Drop for SmtProcess {
    fn drop(&mut self) {
        // send EOF to stdin
        std::mem::drop(self.requests.take());
        if let Err(e) = self.child.wait() {
            panic!("smt process exited with error: {:?}", e);
        }
    }
}

#[cfg(test)]
mod completion_process_tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Writer(Rc<RefCell<Vec<u8>>>);

    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    #[ignore = "requires the explicitly admitted pinned Z3; run in native qualification"]
    fn actual_solver_completion_tracks_response_ownership() {
        let bytes = Rc::new(RefCell::new(Vec::new()));
        let mut process = SmtProcess::launch(&SmtSolver::Z3, Some(Box::new(Writer(bytes.clone()))));
        let request = b"(check-sat)\n".to_vec();
        let pending = process.send_commands_async(request.clone());
        // The real async send transferred response ownership to this handle.
        // Completion must refuse before any footer I/O even if Z3 already replied.
        assert!(pending.smt_process.responses_buf_recv.is_none());
        let before_refusal = bytes.borrow().clone();
        assert!(pending.smt_process.complete_transcript_log().is_err());
        assert_eq!(*bytes.borrow(), before_refusal);
        let response = pending.wait();
        assert_eq!(response, vec!["sat".to_string()]);
        assert!(process.responses_buf_recv.is_some());
        let before_completion = bytes.borrow().clone();
        process.complete_transcript_log().unwrap();
        let mut expected = before_completion.clone();
        expected.extend_from_slice(b";; SAIL-COMPLETE 1 transcript process-used\n");
        assert_eq!(*bytes.borrow(), expected);
        println!("actual request bytes: {:?}", request);
        println!("actual completed response: {:?}", response);
        println!("transcript before refused footer: {:?}", before_refusal);
        println!("transcript after real wait: {:?}", before_completion);
        println!("transcript after completed footer: {:?}", bytes.borrow());
    }
}
