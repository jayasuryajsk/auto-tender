//! Utility functions and modules for the editor.
//!
//! This crate provides utility functions and types that are used across the application.

pub mod instance;

// Re-export common utilities for easy use
pub use instance::{
    cache_dir, config_dir, data_dir, ensure_directories, instance_id, instance_path,
};