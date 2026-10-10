//! 3-way line merge (diff3) of a note edited on this device and remotely since the last agreed text.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge {
    /// The edits did not overlap: the combined text.
    Clean(String),
    /// The edits overlap; the engine keeps the local file and writes the remote text as a conflict copy.
    Conflict,
}

/// Merges `local` and `remote` against `base`. Identical sides and one-sided edits are clean without diffing.
pub fn merge(base: &str, local: &str, remote: &str) -> Merge {
    if local == remote || remote == base {
        return Merge::Clean(local.to_string());
    }
    if local == base {
        return Merge::Clean(remote.to_string());
    }
    match diffy::merge(base, local, remote) {
        Ok(text) => Merge::Clean(text),
        Err(_) => Merge::Conflict,
    }
}
