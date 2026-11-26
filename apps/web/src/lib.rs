//! Browser entry for Brocode: compiles `opencode-desktop` as a library for `wasm32`.

#[cfg(target_arch = "wasm32")]
mod browser_history;
// `#[wasm_bindgen(jspi)]` expands a sibling helper module that calls `jspi_is_experimental`.
#[cfg(target_arch = "wasm32")]
#[allow(deprecated)]
mod web;

#[cfg(target_arch = "wasm32")]
pub use web::start;
