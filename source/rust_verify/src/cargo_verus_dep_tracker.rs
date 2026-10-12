use std::collections::{BTreeMap, BTreeSet};
use std::env::{self, VarError};
use std::path::PathBuf;

use rustc_span::symbol::Symbol;

/// Sail fork (VI-0005): the verification pass's dependencies, for the compile pass.
static SAIL_VERIFY_PASS_DEPINFO: std::sync::Mutex<Option<DepTracker>> = std::sync::Mutex::new(None);

#[derive(Debug)]
pub struct DepTracker {
    env: BTreeMap<String, Option<String>>,
    files: BTreeSet<PathBuf>,
}

impl DepTracker {
    pub fn init() -> Self {
        let mut dep_tracker = DepTracker { env: BTreeMap::new(), files: BTreeSet::new() };

        // Track env vars used by Verus
        dep_tracker.get_env("VERUS_Z3_PATH");
        dep_tracker.get_env("VERUS_SINGULAR_PATH");
        dep_tracker
    }

    pub(crate) fn get_env(&mut self, var: &str) -> Option<String> {
        let val = match env::var(var) {
            Ok(s) => Some(s),
            Err(VarError::NotPresent) => None,
            Err(VarError::NotUnicode(invalid)) => panic!(
                "the value of environment variable {:?} is not value unicode: {:?}",
                var, invalid
            ),
        };
        self.env.insert(var.to_owned(), val.clone());
        val
    }

    pub fn compare_env(&mut self, var: &str, val: &str) -> bool {
        self.get_env(var).as_deref() == Some(val)
    }

    pub(crate) fn mark_file(&mut self, path: PathBuf) {
        self.files.insert(path);
    }

    /// Sail fork (VI-0005): remember the dependencies the verification pass
    /// recorded (this tracker's variables and files, plus everything rustc and
    /// the transitive .vir imports added), so the later erase-macro compile can
    /// record them too.
    ///
    /// Via Cargo, rust_verify runs rustc twice: the verification pass writes
    /// a dep-info file with these `env-dep` lines, then the compile pass
    /// (`run_with_erase_macro_compile`) writes the same `.d` path again
    /// without them. Cargo then never saw the forwarded Verus arguments
    /// (`__VERUS_DRIVER_ARGS__...`), so changing them (`--rlimit`, `--log-dir`,
    /// ...) left a verified crate fresh and it was not re-verified.
    pub(crate) fn sail_record_verify_pass_depinfo(sess: &rustc_session::Session) {
        let env = sess
            .env_depinfo
            .borrow()
            .iter()
            .map(|(var, val)| (var.as_str().to_owned(), val.map(|v| v.as_str().to_owned())))
            .collect();
        let files = sess.file_depinfo.borrow().iter().map(|f| PathBuf::from(f.as_str())).collect();
        *SAIL_VERIFY_PASS_DEPINFO.lock().expect("dep-info lock") = Some(DepTracker { env, files });
    }

    /// Sail fork (VI-0005): install the dependencies recorded by
    /// `sail_record_verify_pass_depinfo`, if any, into the compile pass.
    pub(crate) fn sail_install_verify_pass_depinfo(config: &mut rustc_interface::Config) {
        if let Some(recorded) = SAIL_VERIFY_PASS_DEPINFO.lock().expect("dep-info lock").take() {
            recorded.config_install(config);
        }
    }

    pub(crate) fn config_install(self, config: &mut rustc_interface::Config) {
        config.track_state = Some(Box::new(move |sess| {
            for (var, val) in self.env.iter() {
                sess.env_depinfo
                    .borrow_mut()
                    .insert((Symbol::intern(var), val.as_deref().map(Symbol::intern)));
            }
            for path in self.files.iter() {
                sess.file_depinfo.borrow_mut().insert(Symbol::intern(
                    path.to_str().unwrap_or_else(|| panic!("path {:?} is not valid unicode", path)),
                ));
            }
        }));
    }
}
