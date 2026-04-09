/// Logs an informational message to the appropriate console depending on the target platform.
/// In WASM builds, this logs to the browser console; otherwise, it prints to stdout.
/// # Arguments
/// * `msg` - The message to log.
pub fn log_info(msg: String) {
    #[cfg(all(feature = "wasm", target_arch = "wasm32"))]
    {
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&msg));
    }
    #[cfg(not(all(feature = "wasm", target_arch = "wasm32")))]
    {
        println!("{}", msg);
    }
}
