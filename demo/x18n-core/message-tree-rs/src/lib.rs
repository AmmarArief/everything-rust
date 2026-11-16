//! JS bindings for `packages/core/src/message-tree.ts`: generated from the signatures in `port.rs` by carcinize.
//! Regenerate with `carcinize refresh` after changing them.

use wasm_bindgen::prelude::*;

pub mod js;
pub mod port;
pub mod utf16;

/// `isMessageLeaf`.
fn is_message_leaf_inner(value: String) -> Result<bool, String> {
    let value = crate::js::from_json(&value)?;
    let out = port::is_message_leaf(value);
    Ok(out)
}

#[wasm_bindgen(js_name = "isMessageLeaf")]
pub fn is_message_leaf(value: String) -> Result<bool, JsError> {
    is_message_leaf_inner(value).map_err(|e| JsError::new(&e))
}

/// `isMessageTree`.
fn is_message_tree_inner(value: String) -> Result<bool, String> {
    let value = crate::js::from_json(&value)?;
    let out = port::is_message_tree(value);
    Ok(out)
}

#[wasm_bindgen(js_name = "isMessageTree")]
pub fn is_message_tree(value: String) -> Result<bool, JsError> {
    is_message_tree_inner(value).map_err(|e| JsError::new(&e))
}

/// `pathId`.
fn path_id_inner(path: Vec<String>) -> Result<String, String> {
    let out = port::path_id(path);
    Ok(out)
}

#[wasm_bindgen(js_name = "pathId")]
pub fn path_id(path: Vec<String>) -> Result<String, JsError> {
    path_id_inner(path).map_err(|e| JsError::new(&e))
}

/// `applyMessageOps`.
fn apply_message_ops_inner(tree: String, ops: String) -> Result<String, String> {
    let tree = crate::js::from_json(&tree)?;
    let ops = crate::js::from_json(&ops)?;
    let out = port::apply_message_ops(tree, ops)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "applyMessageOps")]
pub fn apply_message_ops(tree: String, ops: String) -> Result<String, JsError> {
    apply_message_ops_inner(tree, ops).map_err(|e| JsError::new(&e))
}

/// `buildTree`.
fn build_tree_inner(leaves: String) -> Result<String, String> {
    let leaves = crate::js::from_json(&leaves)?;
    let out = port::build_tree(leaves)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "buildTree")]
pub fn build_tree(leaves: String) -> Result<String, JsError> {
    build_tree_inner(leaves).map_err(|e| JsError::new(&e))
}
