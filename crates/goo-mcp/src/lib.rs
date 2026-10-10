//! goo's MCP server (Model Context Protocol), so AI assistants can read, search and edit notes.
//!
//! - [`tools`]: the note tools and the [`tools::Host`] trait the app implements (open folder, recent folders, unsaved
//!   notes, change notifications).
//! - [`protocol`]: JSON-RPC handling (`initialize`, `tools/list`, `tools/call`, `ping`).
//! - [`http`]: the endpoint inside the app, `POST http://127.0.0.1:<port>/mcp`, token-protected.
//! - [`bridge`]: `goo --mcp-stdio`, the command assistants launch; relays stdin/stdout to the endpoint and starts goo
//!   when needed.
//! - [`files`]: settings, `endpoint.json` and `launch.json` shared by the two sides.
//!
//! Same design as Slinger's MCP server (Streamable HTTP endpoint in the app + a stdio bridge), adapted to Rust.

pub mod bridge;
pub mod files;
pub mod http;
pub mod protocol;
pub mod tools;

pub use tools::Host;

/// The arguments clients use to start the bridge: `goo --mcp-stdio`.
pub const STDIO_FLAG: &str = "--mcp-stdio";

#[cfg(test)]
mod tests;
