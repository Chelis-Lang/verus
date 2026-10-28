use std::collections::{BTreeMap, BTreeSet};
use std::env::{self, VarError};
use std::path::PathBuf;

use rustc_span::symbol::Symbol;

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

    pub(crate) fn config_install(mut self, config: &mut rustc_interface::Config) {
        // Keep the driver's own declaration separate from actual source and
        // macro reads. Inserting it before expansion would deduplicate an
        // explicit env!/option_env! read of the same value (Sail #551).
        let common = self.env.remove(crate::cargo_verus::VERUS_DRIVER_ARGS);
        config.track_state = Some(Box::new(move |sess| {
            crate::sail_compiler_inputs::driver_common(common);
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
