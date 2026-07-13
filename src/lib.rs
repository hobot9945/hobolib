//! `hobolib` - General-purpose utility library.
//!
//! This crate contains shared tools, macros, and helpers used across various projects.
//!
//! # Platform Support
//! Initial development and testing are primarily focused on the Windows platform.
//! However, the library is designed with cross-platform compatibility in mind,
//! and extending full support to Linux is planned for the future.

pub mod glob;
pub mod macros;
pub mod misc;
pub mod automation;
pub mod clipboard;
pub mod keyboard;
pub mod markdown_fence;
pub mod mouse;
pub mod opencv;
pub mod screenshot;
pub mod window;
