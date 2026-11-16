//! carcinize: plan and check incremental migrations to Rust.
//!
//! The core (`analyze`, `report`, `risks`) is language-neutral in shape; each source
//! language is a plugin module. TypeScript/JavaScript (`ts`) is the first.

pub mod analyze;
pub mod report;
pub mod risks;
pub mod ts;
