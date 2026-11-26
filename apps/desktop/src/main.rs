//! Thin native binary entrypoint for Brocode Desktop.
//!
//! All shared application logic lives in [`opencode_desktop`] (`src/lib.rs`) so both the native
//! desktop binary and `apps/web` (`wasm32`) compile the same library target without manifest
//! duplication.

/// Counts each thread's allocations for the frame diagnostics and the benchmarks, over mimalloc:
/// the system heap costs a Luau plugin's rebuild (its tables, strings and elements) about a fifth
/// more on Windows.
#[cfg(all(not(target_arch = "wasm32"), not(code_layers)))]
#[global_allocator]
static ALLOC: procwatch::alloc::Counting<opencode_desktop::OomGuard<mimalloc::MiMalloc>> =
    procwatch::alloc::Counting::new(opencode_desktop::OomGuard(mimalloc::MiMalloc));

// A macOS build with its code as several libraries (`apps/desktop/layers/layers.txt`): naming them
// makes the exe link them, and the allocator above is the lowest library's (`layers/base.rs`).
#[cfg(code_layers)]
include!("../layers/gen/all.rs");

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    #[cfg(code_layers)]
    opencode_layer_base::on_alloc_failure(opencode_desktop::alloc_failed);
    opencode_desktop::run_native();
}

#[cfg(target_arch = "wasm32")]
fn main() {}
