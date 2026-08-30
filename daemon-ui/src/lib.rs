use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    dioxus::launch(App);
    Ok(())
}

#[component]
pub fn App() -> Element {
    rsx! {
        div { "Daemon UI" }
    }
}
