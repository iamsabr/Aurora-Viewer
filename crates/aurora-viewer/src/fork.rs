//! What this fork's builds do differently from the official viewer. The fork
//! (see FORK.md at the repository root) is built and used on its own, never
//! sent upstream, so anything that would reach the official project's
//! services stays on this PC instead. Everything the fork changes for that
//! goes through this module, so that an upstream release merges in with as
//! few conflicts as possible.
//!
//! The updater needs nothing here: it only checks for updates in a build whose
//! version is one the official Release workflow publishes (a stable version or
//! a `beta` pre-release, with no Git distance after it; `updater::checks_enabled`).
//! A fork release is tagged `vX.Y.Z-fork.N`, which is neither, so a fork build
//! never checks for, offers or installs an official release.

use crate::settings::CrashReports;

/// The crash window's sending mode in a fork build: the report stays on this
/// PC whatever the setting says, since the official issue tracker the window
/// files on has never seen the fork's code. The window still opens the report
/// folder and copies the trace, to share by hand.
pub fn crash_reports(_setting: CrashReports) -> CrashReports {
    CrashReports::Never
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fork_build_never_sends_a_crash_report() {
        for setting in [CrashReports::Ask, CrashReports::Always, CrashReports::Never] {
            assert_eq!(crash_reports(setting), CrashReports::Never, "{setting:?}");
        }
    }

    /// A fork release tag is not a version the official updater serves, so a
    /// fork build never checks for updates (`updater::checks_enabled`).
    #[test]
    fn a_fork_version_is_not_one_the_updater_serves() {
        use aurora_updater::feed::{Channel, version};
        for fork in ["0.9.14-fork.1", "0.9.15-fork.3", "0.9.14-fork.1+2.g0123456789"] {
            let parsed = version(fork).expect("a fork version parses");
            assert!(!Channel::Beta.accepts(&parsed) && !Channel::Stable.accepts(&parsed), "{fork}");
        }
        // what the official Release workflow publishes is served
        for official in ["0.9.14", "0.9.15-beta.20261201.1"] {
            assert!(Channel::Beta.accepts(&version(official).expect("parses")), "{official}");
        }
    }
}
