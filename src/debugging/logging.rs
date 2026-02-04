/// Logs an informational message to the appropriate console depending on the target platform.
/// In WASM builds, this logs to the browser console; otherwise, it prints to stdout.
/// # Arguments
/// * `msg` - The message to log.
pub fn log_info(msg: String) {
    #[cfg(feature = "wasm")]
    {
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&msg));
    }
    #[cfg(not(feature = "wasm"))]
    {
        println!("{}", msg);
    }
}
