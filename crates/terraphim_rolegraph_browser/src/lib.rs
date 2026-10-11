//! Single-owner JSON bridge to the real RoleGraph. Browser-safe upstream release pending.
mod bounded_json;
mod checkpoint;
mod session;
mod validation;
mod wire;

pub use session::GraphSession;
use wasm_bindgen::prelude::*;
pub use wire::BridgeError;

#[wasm_bindgen]
#[derive(Default)]
pub struct BrowserRoleGraph {
    session: GraphSession,
}

#[wasm_bindgen]
impl BrowserRoleGraph {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn dispatch(&mut self, request_json: &str) -> Result<String, JsValue> {
        self.session
            .dispatch(request_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
