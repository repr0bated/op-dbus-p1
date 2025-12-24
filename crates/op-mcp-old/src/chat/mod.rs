//! Chat module - The Brains of the Project
//!
//! This module consolidates the chat server, orchestration, D-Bus control,
//! and introspection into a single cohesive unit.

pub mod dbus_control;
pub mod introspection;
pub mod introspection_parser;
pub mod orchestrator;
pub mod server;
