//! [`bake`](crate::bake) for the page, compiled to WebAssembly.

use js_sys::{Function, Uint8Array};
use wasm_bindgen::prelude::*;
use xodr::opencrg::CrgGrid;

/// Bake an OpenDRIVE document into its scene JSON. `crg` is called with the
/// file a `<CRG file>` names and returns its bytes, or `undefined` when the
/// user did not open it.
#[wasm_bindgen]
pub fn bake(xodr: &str, crg: &Function) -> Result<String, JsError> {
    let scene = crate::bake(xodr, |file| {
        let bytes = crg
            .call1(&JsValue::NULL, &file.into())
            .map_err(|e| format!("{e:?}"))?;
        if bytes.is_undefined() {
            return Err("not among the opened files".into());
        }
        CrgGrid::from_bytes(&Uint8Array::new(&bytes).to_vec()).map_err(|e| e.to_string())
    })
    .map_err(|e| JsError::new(&e))?;
    let mut out = Vec::new();
    scene
        .write(&mut out)
        .map_err(|e| JsError::new(&e.to_string()))?;
    String::from_utf8(out).map_err(|e| JsError::new(&e.to_string()))
}
