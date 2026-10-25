//! goo's in-app updates (no Tauri dependency, so it is tested on its own):
//!
//! - [`release`]: GitHub's release JSON, read defensively (stable versions only, links built from the version).
//! - [`check`]: "is there a newer goo?", one GET to the latest-release endpoint.
//! - [`detect`]: how this installation can update itself (AppImage, .deb, Windows setup, macOS bundle) or why not.
//! - [`installer`]: download the matching installer, verify size and SHA-256, install it and restart.
//!
//! Same design as Slinger's `electron/services/updateCheck.ts` and `updateInstall.ts`.

pub mod check;
pub mod detect;
pub mod installer;
pub mod release;

pub use check::{check, feed_url, CheckError, CheckResult};
pub use detect::{asset_name, detect_install, DetectInput, InstallMethod, InstallTarget, Probe, SystemProbe};
pub use installer::{Deps, Installer, Phase, Spawner, SystemSpawner, UpdateState};
pub use release::{is_newer, ReleaseInfo, LATEST_RELEASE_API, RELEASES_REPO};

#[cfg(test)]
mod tests;
