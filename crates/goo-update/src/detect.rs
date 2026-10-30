//! How this installation updates itself, or why it cannot. Decided from how goo was installed, never from what the
//! release says: an AppImage, the Linux `.deb`, the Windows NSIS installer, or a macOS app bundle. Development builds,
//! `.rpm` and `.msi` installs and read-only locations keep the manual "View release" path.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallMethod {
    AppImage,
    Deb,
    Nsis,
    Dmg,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallTarget {
    pub method: Option<InstallMethod>,
    /// Why not, when `method` is None.
    pub reason: Option<String>,
    /// The AppImage file, or the `.app` bundle (macOS).
    pub target: Option<PathBuf>,
}

impl InstallTarget {
    fn yes(method: InstallMethod, target: Option<PathBuf>) -> Self {
        InstallTarget { method: Some(method), reason: None, target }
    }
    fn no(reason: impl Into<String>) -> Self {
        InstallTarget { method: None, reason: Some(reason.into()), target: None }
    }
}

/// The questions about the machine, so tests can answer them.
pub trait Probe {
    fn is_writable(&self, dir: &Path) -> bool;
    fn exists(&self, path: &Path) -> bool;
    fn has_command(&self, name: &str) -> bool;
}

pub struct DetectInput<'a> {
    /// `std::env::consts::OS`: "linux", "macos", "windows".
    pub os: &'a str,
    /// `std::env::consts::ARCH`: "x86_64", "aarch64".
    pub arch: &'a str,
    pub is_packaged: bool,
    /// `$APPIMAGE`
    pub appimage: Option<&'a Path>,
    pub exec_path: &'a Path,
    pub probe: &'a dyn Probe,
}

/// The Linux package and command (Debian/Ubuntu ship an unrelated `goo`; see src-tauri/tauri.linux.conf.json).
pub const DEB_BINARY: &str = "/usr/bin/goo-notes";
pub const DEB_LIST: &str = "/var/lib/dpkg/info/goo-notes.list";

pub fn detect_install(i: &DetectInput) -> InstallTarget {
    if !i.is_packaged {
        return InstallTarget::no("This is a development build: update it from the source, or install a release.");
    }
    let arch_ok = matches!(i.arch, "x86_64" | "aarch64");
    match i.os {
        "linux" => {
            if !arch_ok {
                return InstallTarget::no("Releases are built for x86-64 and ARM64 Linux only.");
            }
            if let Some(image) = i.appimage {
                let dir = image.parent().unwrap_or(Path::new("/"));
                return if i.probe.is_writable(dir) {
                    InstallTarget::yes(InstallMethod::AppImage, Some(image.to_path_buf()))
                } else {
                    InstallTarget::no(format!("The folder of the AppImage ({}) is not writable.", dir.display()))
                };
            }
            if i.exec_path == Path::new(DEB_BINARY) && i.probe.exists(Path::new(DEB_LIST)) {
                return if i.probe.has_command("pkexec") && i.probe.has_command("apt-get") {
                    InstallTarget::yes(InstallMethod::Deb, None)
                } else {
                    InstallTarget::no("Installing the .deb needs pkexec (polkit) and apt-get, which are not both available.")
                };
            }
            InstallTarget::no("This copy of goo was not installed from the .deb or the AppImage (an .rpm is updated from the release page).")
        }
        "windows" => {
            if i.arch != "x86_64" {
                return InstallTarget::no("Releases are built for 64-bit Windows (x64) only.");
            }
            // The NSIS installer leaves uninstall.exe next to goo.exe; an .msi install does not.
            let dir = i.exec_path.parent().unwrap_or(Path::new(""));
            if i.probe.exists(&dir.join("uninstall.exe")) {
                InstallTarget::yes(InstallMethod::Nsis, None)
            } else {
                InstallTarget::no("This copy of goo was not installed with the setup .exe (an .msi is updated from the release page).")
            }
        }
        "macos" => {
            if !arch_ok {
                return InstallTarget::no("Releases are built for Apple silicon and Intel Macs only.");
            }
            let path = i.exec_path.to_string_lossy();
            let Some(at) = path.find(".app/") else {
                return InstallTarget::no("goo is not running from an app bundle.");
            };
            let bundle = PathBuf::from(&path[..at + 4]);
            let dir = bundle.parent().unwrap_or(Path::new("/"));
            if i.probe.is_writable(dir) {
                InstallTarget::yes(InstallMethod::Dmg, Some(bundle))
            } else {
                InstallTarget::no(format!("The folder of {} ({}) is not writable.", bundle.file_name().and_then(|n| n.to_str()).unwrap_or("goo.app"), dir.display()))
            }
        }
        other => InstallTarget::no(format!("Releases are not built for {other}.")),
    }
}

/// The release asset this installation needs (the names `scripts/release-assets.mjs` gives them).
pub fn asset_name(method: InstallMethod, version: &str, arch: &str) -> String {
    let arm = arch == "aarch64";
    match method {
        InstallMethod::AppImage => format!("goo-{version}-linux-{}.AppImage", if arm { "aarch64" } else { "x86_64" }),
        InstallMethod::Deb => format!("goo-{version}-linux-{}.deb", if arm { "arm64" } else { "amd64" }),
        InstallMethod::Nsis => format!("goo-{version}-win-x64.exe"),
        InstallMethod::Dmg => format!("goo-{version}-mac-{}.dmg", if arm { "arm64" } else { "x64" }),
    }
}

/// The real answers.
pub struct SystemProbe;

impl Probe for SystemProbe {
    fn is_writable(&self, dir: &Path) -> bool {
        // create-and-remove is the only portable test that also respects read-only mounts and ACLs
        let probe = dir.join(format!(".goo-write-test-{}", std::process::id()));
        match std::fs::File::create(&probe) {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
                true
            }
            Err(_) => false,
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn has_command(&self, name: &str) -> bool {
        let Some(paths) = std::env::var_os("PATH") else { return false };
        std::env::split_paths(&paths).any(|d| d.join(name).is_file())
    }
}
