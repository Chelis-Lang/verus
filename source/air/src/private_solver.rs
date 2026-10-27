//! Explicit solver-child settings retained in compiler process memory.
//!
//! This is process-construction data, not authentication or proof authority.
//! No environment variable, argument, file or registration installs it.

use crate::context::SmtSolver;
use std::ffi::OsString;
use std::process::Command;

/// An explicit executable and ordered environment operations for its solver
/// child alone. Native paths, values, absence and empty values stay distinct.
#[derive(Clone, Debug)]
pub struct PrivateSolverLaunch {
    executable: OsString,
    environment: Vec<(OsString, Option<OsString>)>,
}

impl PrivateSolverLaunch {
    /// Retain an executable without consulting an ambient selector.
    pub fn new(executable: OsString) -> Self {
        Self { executable, environment: Vec::new() }
    }

    /// Record an override in its original order for the eventual solver child.
    pub fn set_environment(&mut self, name: OsString, value: OsString) -> &mut Self {
        self.environment.push((name, Some(value)));
        self
    }

    /// Record removal for the eventual solver child, preserving absence.
    pub fn remove_environment(&mut self, name: OsString) -> &mut Self {
        self.environment.push((name, None));
        self
    }
}

/// The actual solver command factory. Ordinary resolution is called only when
/// no explicit configuration was supplied, retaining its existing selection
/// policy. Explicit configuration bypasses it. Process globals are not changed.
pub(crate) fn command(
    solver: &SmtSolver,
    private: Option<&PrivateSolverLaunch>,
    ordinary_executable: impl FnOnce() -> OsString,
) -> Command {
    let executable = match private {
        Some(configuration) => configuration.executable.clone(),
        None => ordinary_executable(),
    };
    let mut command = Command::new(executable);
    command.args(match solver {
        SmtSolver::Z3 => vec!["-smt2", "-in"],
        SmtSolver::Cvc5 => vec![
            "--no-interactive",
            "--produce-models",
            "--quant-dsplit=none",
            "--no-cbqi",
            "--user-pat=strict",
        ],
    });
    if let Some(configuration) = private {
        for (name, value) in &configuration.environment {
            match value {
                Some(value) => {
                    command.env(name, value);
                }
                None => {
                    command.env_remove(name);
                }
            }
        }
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;

    #[test]
    fn ordinary_selection_and_both_original_argument_lists_are_retained() {
        for (solver, arguments) in [
            (SmtSolver::Z3, vec!["-smt2", "-in"]),
            (
                SmtSolver::Cvc5,
                vec![
                    "--no-interactive",
                    "--produce-models",
                    "--quant-dsplit=none",
                    "--no-cbqi",
                    "--user-pat=strict",
                ],
            ),
        ] {
            let calls = Cell::new(0);
            let child = command(&solver, None, || {
                calls.set(calls.get() + 1);
                "ordinary-program".into()
            });
            assert_eq!(calls.get(), 1);
            assert_eq!(child.get_program(), OsStr::new("ordinary-program"));
            assert_eq!(
                child.get_args().collect::<Vec<_>>(),
                arguments.into_iter().map(OsStr::new).collect::<Vec<_>>()
            );
            assert_eq!(child.get_envs().count(), 0);
        }
    }

    #[test]
    fn explicit_configuration_changes_only_the_owned_solver_command() {
        let mut compiler = Command::new("original-verus");
        compiler.arg("original.rs").env("OBSERVED", "caller");
        let before = compiler
            .get_envs()
            .map(|(key, value)| (key.to_owned(), value.map(OsStr::to_owned)))
            .collect::<Vec<_>>();
        let mut configuration = PrivateSolverLaunch::new("private-shim".into());
        configuration
            .set_environment("OBSERVED".into(), "solver-only".into())
            .set_environment("EMPTY".into(), "first".into())
            .remove_environment("EMPTY".into())
            .set_environment("EMPTY".into(), "".into())
            .remove_environment("ABSENT".into());
        let calls = Cell::new(0);
        let solver = command(&SmtSolver::Z3, Some(&configuration), || {
            calls.set(calls.get() + 1);
            "ordinary-program".into()
        });
        assert_eq!(calls.get(), 0);
        assert_eq!(solver.get_program(), OsStr::new("private-shim"));
        assert_eq!(
            solver.get_args().collect::<Vec<_>>(),
            vec![OsStr::new("-smt2"), OsStr::new("-in")]
        );
        let environment = solver
            .get_envs()
            .map(|(key, value)| (key.to_owned(), value.map(OsStr::to_owned)))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(environment.get(OsStr::new("OBSERVED")), Some(&Some("solver-only".into())));
        assert_eq!(environment.get(OsStr::new("EMPTY")), Some(&Some("".into())));
        assert_eq!(environment.get(OsStr::new("ABSENT")), Some(&None));
        assert_eq!(compiler.get_program(), OsStr::new("original-verus"));
        assert_eq!(compiler.get_args().collect::<Vec<_>>(), vec![OsStr::new("original.rs")]);
        assert_eq!(
            compiler
                .get_envs()
                .map(|(key, value)| (key.to_owned(), value.map(OsStr::to_owned)))
                .collect::<Vec<_>>(),
            before
        );
    }

    #[cfg(unix)]
    #[test]
    fn explicit_executable_and_environment_retain_non_unicode_bytes() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let program = OsString::from_vec(vec![b'/', b'z', 0xff]);
        let value = OsString::from_vec(vec![0xfe, b'=', b'\n']);
        let mut configuration = PrivateSolverLaunch::new(program.clone());
        configuration.set_environment("SAIL_Z3_PATH".into(), value.clone());
        let child = command(&SmtSolver::Z3, Some(&configuration), || "unused".into());
        assert_eq!(child.get_program().as_bytes(), program.as_bytes());
        assert_eq!(
            child.get_envs().next(),
            Some((OsStr::new("SAIL_Z3_PATH"), Some(value.as_os_str())))
        );
    }
}
