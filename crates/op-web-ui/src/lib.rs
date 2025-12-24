//! Pure Rust WebAssembly Frontend for op-dbus
//!
//! This is a Leptos-based frontend that:
//! - Compiles to WebAssembly
//! - Connects to the real op-web backend APIs
//! - Provides natural language server administration
//! - Shows real system status (not mock data)

use leptos::*;
use leptos_router::*;

mod api;
mod components;
mod pages;
mod state;

pub use api::*;
pub use components::*;
pub use pages::*;
pub use state::*;

/// Main application component
#[component]
pub fn App() -> impl IntoView {
    // Initialize tracing for WASM
    tracing_wasm::set_as_global_default();

    // Create global application state
    let app_state = create_rw_signal(AppState::new());
    provide_context(app_state);

    view! {
        <Router>
            <main class="app-container">
                <Header/>
                <Routes>
                    <Route path="/" view=ChatPage/>
                    <Route path="/tools" view=ToolsPage/>
                    <Route path="/status" view=StatusPage/>
                    <Route path="/settings" view=SettingsPage/>
                </Routes>
            </main>
        </Router>
    }
}

/// Application entry point for WASM
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
