//! carcinize: plan, bridge, verify and ship incremental migrations to Rust.
//!
//! The steps (`analyze`, `bridge`, `harness`, `shadow`, `gate`) are language-neutral in shape;
//! each source language is a plugin module. TypeScript/JavaScript (`ts`) is the first.

pub mod analyze;
pub mod bridge;
pub mod codegen;
pub mod codegen_c;
pub mod codegen_ffi;
pub mod codegen_go;
pub mod codegen_java;
pub mod codegen_py;
pub mod codegen_ruby;
pub mod codegen_tswire;
pub mod gate;
pub mod harness;
pub mod harness_c;
pub mod harness_go;
pub mod harness_java;
pub mod harness_py;
pub mod harness_ruby;
pub mod lang;
pub mod mcp;
pub mod report;
pub mod risks;
pub mod shadow;
pub mod ts;
pub mod wire;
