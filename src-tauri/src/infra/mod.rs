//! Infrastructure adapters: `db`, `mcp` (MCP client over HTTP), `obsidian` (RF-02), `github` (RF-03) and `secrets` (credential store) are implemented.

pub mod claude;
pub mod db;
pub mod git;
pub mod github;
pub mod mcp;
pub mod obsidian;
pub mod process;
pub mod secrets;
