//! carcinize: plan, bridge, verify and ship incremental migrations to Rust.
//!
//! The steps (`analyze`, `bridge`, `harness`, `shadow`, `gate`) are language-neutral in shape;
//! each source language is a plugin module. TypeScript/JavaScript (`ts`) is the first.

pub mod analyze;
pub mod bridge;
pub mod gate;
pub mod harness;
pub mod mcp;
pub mod report;
pub mod risks;
pub mod shadow;
pub mod ts;
