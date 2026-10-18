//! [`bake`](crate::bake) for the page, compiled to WebAssembly.

use js_sys::{Function, Uint8Array};
use wasm_bindgen::prelude::*;
use xodr::opencrg::{CrgGrid, LoadOptions};

/// Bake an OpenDRIVE document into its scene JSON. `files` is called with
/// the file an `<include>`, a `<CRG file>` or a CRG's `$ROAD_CRG_FILE` names
/// and returns its bytes, or `undefined` or `null` when the user did not
/// open it.
#[wasm_bindgen]
pub fn bake(xodr: &str, files: &Function) -> Result<String, JsError> {
    let opened = |file: &str| -> Result<Vec<u8>, String> {
        let bytes = files
            .call1(&JsValue::NULL, &file.into())
            .map_err(|e| format!("{e:?}"))?;
        if bytes.is_undefined() || bytes.is_null() {
            return Err("not among the opened files".into());
        }
        Ok(Uint8Array::new(&bytes).to_vec())
    };
    let include = |file: &str| String::from_utf8(opened(file)?).map_err(|e| e.to_string());
    let crg = |file: &str| {
        CrgGrid::from_bytes_with(&opened(file)?, &LoadOptions::default(), |name| {
            opened(name).map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e))
        })
        .map_err(|e| e.to_string())
    };
    let scene = crate::bake(xodr, include, crg).map_err(|e| JsError::new(&e))?;
    let mut out = Vec::new();
    scene
        .write(&mut out)
        .map_err(|e| JsError::new(&e.to_string()))?;
    String::from_utf8(out).map_err(|e| JsError::new(&e.to_string()))
}
