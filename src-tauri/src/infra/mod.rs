//! Infrastructure adapters: `db`, `mcp` (MCP client over HTTP) and `obsidian` (RF-02) are implemented.

pub mod claude;
pub mod db;
pub mod git;
pub mod github;
pub mod mcp;
pub mod obsidian;
pub mod process;
pub mod secrets;
