//! An installed file that has drifted from the source it was built from.
//!
//! Substitution is what makes a plain diff useless: a template carries
//! `__HARNESS_DIR__` where the installed copy carries `.harness`, so comparing
//! the trees raw reports every file forever. The comparison is therefore
//! against what the installer would write now, which means it needs the
//! installer -- `init::install(root, InitOpts { dry_run: true, .. })`, Task 13.
//! Until that lands this reports nothing rather than guessing, and Task 13's
//! own test (`an installed file that drifted from its source is reported`) is
//! what turns it on.

use super::{ProbeCtx, ProbeResult};

pub fn probe(_ctx: &ProbeCtx) -> ProbeResult {
    ProbeResult::Count(vec![])
}
