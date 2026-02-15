//! Tests for panic=abort mode.
//!
//! These tests verify the library works correctly when panics abort
//! rather than unwind. We can only test non-panicking paths here.
//!
//! Run with: cargo test --test abort --target wasm32-unknown-unknown \
//!           --config 'target.wasm32-unknown-unknown.rustflags=["-Cpanic=abort"]'

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

use wasm_signal_handler::{
    clear_signal, clear_signal_handler, peek_signal, set_signal, set_signal_handler,
    try_check_signal, Signal, SIGNAL,
};

wasm_bindgen_test_configure!(run_in_node_experimental);

// ============================================================================
// JavaScript helper functions
// ============================================================================

#[wasm_bindgen(inline_js = r#"
export function readSignalValue(wasmMemory, signalAddr) {
    const view = new DataView(wasmMemory.buffer);
    return view.getUint32(signalAddr, true);
}

export function writeSignalValue(wasmMemory, signalAddr, value) {
    const view = new DataView(wasmMemory.buffer);
    view.setUint32(signalAddr, value, true);
}
"#)]
extern "C" {
    fn readSignalValue(memory: &JsValue, signal_addr: u32) -> u32;
    fn writeSignalValue(memory: &JsValue, signal_addr: u32, value: u32);
}

fn get_wasm_memory() -> JsValue {
    wasm_bindgen::memory()
}

fn get_signal_addr() -> u32 {
    (&SIGNAL) as *const _ as u32
}

// ============================================================================
// Tests: These must NOT trigger any panics (panic=abort would terminate)
// ============================================================================

#[wasm_bindgen_test]
fn test_no_signal_ok() {
    clear_signal();
    clear_signal_handler();

    // No signal set - should return Ok
    assert!(try_check_signal().is_ok());
}

#[wasm_bindgen_test]
fn test_signal_returns_err() {
    clear_signal();
    clear_signal_handler();

    set_signal(42);

    // Should return Err, not panic
    let result = try_check_signal();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().0, 42);
}

#[wasm_bindgen_test]
fn test_handler_clears_signal_no_panic() {
    clear_signal();

    // Handler that clears the signal
    set_signal_handler(|_signal| Ok(()));

    set_signal(123);

    // Handler returns Ok, so no panic path triggered
    let result = try_check_signal();
    assert!(result.is_ok());

    clear_signal_handler();
}

#[wasm_bindgen_test]
fn test_handler_propagates_error_no_panic() {
    clear_signal();

    // Handler that propagates the error
    set_signal_handler(|signal| Err(Signal(signal.0 * 2)));

    set_signal(21);

    // try_check_signal returns Err, doesn't panic
    let result = try_check_signal();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().0, 42);

    clear_signal_handler();
}

#[wasm_bindgen_test]
fn test_js_write_signal_try_check() {
    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Set signal from JS
    writeSignalValue(&memory, signal_addr, 999);

    // Use try_check (doesn't panic)
    let result = try_check_signal();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().0, 999);
}

#[wasm_bindgen_test]
fn test_js_read_signal() {
    clear_signal();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    set_signal(456);

    let js_value = readSignalValue(&memory, signal_addr);
    assert_eq!(js_value, 456);

    clear_signal();
}

#[wasm_bindgen_test]
fn test_peek_does_not_clear() {
    clear_signal();
    clear_signal_handler();

    set_signal(100);

    // Peek multiple times
    assert_eq!(peek_signal(), Some(Signal(100)));
    assert_eq!(peek_signal(), Some(Signal(100)));
    assert_eq!(peek_signal(), Some(Signal(100)));

    // Still there
    let result = try_check_signal();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().0, 100);

    // Now cleared
    assert!(peek_signal().is_none());
}

#[wasm_bindgen_test]
fn test_clear_signal_returns_value() {
    clear_signal();

    set_signal(777);
    let cleared = clear_signal();
    assert_eq!(cleared, Some(Signal(777)));

    // Already cleared
    let cleared_again = clear_signal();
    assert_eq!(cleared_again, None);
}

#[wasm_bindgen_test]
fn test_signal_addr_valid() {
    let addr = get_signal_addr();
    assert!(addr > 0);
    assert!(addr < 0x1000000);
}

#[wasm_bindgen_test]
fn test_multiple_signals_try_check() {
    clear_signal();
    clear_signal_handler();

    for i in 1..=10 {
        set_signal(i);
        let result = try_check_signal();
        assert_eq!(result.unwrap_err().0, i);
        assert!(try_check_signal().is_ok());
    }
}
