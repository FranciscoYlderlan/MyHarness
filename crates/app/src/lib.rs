//! MyHarness core services and infrastructure adapters, without Tauri.
//!
//! - [`infra`]: the adapters (database, Claude CLI, Obsidian CLI, GitHub, MCP, processes,
//!   credential store).
//! - [`nucleo`] and [`integracoes`]: the core that the IPC commands call (UC-01, RN-08).
//! - [`dto`], [`erro`] and [`eventos`]: the types that cross the IPC bridge. The
//!   `src-tauri` crate exports them to TypeScript (`src/bindings/`).
//! - [`canal`]: how the core sends ordered messages to the screen, without the Tauri
//!   `Channel` (the desktop shell gives the implementation).
//!
//! The tests of this crate run without GTK and without WebKit.

pub mod canal;
pub mod dto;
pub mod erro;
pub mod eventos;
pub mod infra;
pub mod integracoes;
pub mod nucleo;
