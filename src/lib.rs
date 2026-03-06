#![doc = include_str!("../README.md")]
//! `expman`: High-performance experiment manager
//!
//! Exposes a core library, CLI, Axum server + embedded frontend, and Python bindings.

#[path = "core/mod.rs"]
pub mod core; // core logic

#[cfg(all(feature = "server", not(target_arch = "wasm32")))]
pub mod api;
#[cfg(target_arch = "wasm32")]
pub mod app;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "python")]
pub mod wrappers;
