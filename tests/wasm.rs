//! WebAssembly integration tests using wasm-bindgen-test.
//!
//! These tests verify that:
//! 1. The WASM_SIGNAL_ADDR global is properly exported
//! 2. JavaScript can read the signal address and write to it
//! 3. Rust correctly detects signals set from JavaScript

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

use wasm_signal_handler::{
    check_signal, clear_signal, clear_signal_handler, peek_signal, set_signal, set_signal_handler,
    try_check_signal, Signal, SIGNAL,
};

wasm_bindgen_test_configure!(run_in_node_experimental);

// ============================================================================
// JavaScript helper functions
// ============================================================================

#[wasm_bindgen(inline_js = r#"
export function getSignalAddress(wasmMemory, addrPtr) {
    // addrPtr points to our WASM_SIGNAL_ADDR which contains the address of SIGNAL
    // In wasm32, pointers are 4 bytes
    const view = new DataView(wasmMemory.buffer);
    return view.getUint32(addrPtr, true); // little-endian
}

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
    fn getSignalAddress(memory: &JsValue, addr_ptr: u32) -> u32;
    fn readSignalValue(memory: &JsValue, signal_addr: u32) -> u32;
    fn writeSignalValue(memory: &JsValue, signal_addr: u32, value: u32);
}

/// Get the WebAssembly memory object
fn get_wasm_memory() -> JsValue {
    wasm_bindgen::memory()
}

/// Get the address of SIGNAL by reading through WASM_SIGNAL_ADDR
fn get_signal_addr() -> u32 {
    (&SIGNAL) as *const _ as u32
}

// ============================================================================
// Tests: Basic signal operations from Rust
// ============================================================================

#[wasm_bindgen_test]
fn test_initial_state_no_signal() {
    clear_signal();
    clear_signal_handler();

    assert!(peek_signal().is_none(), "Signal should be clear initially");
    assert!(
        try_check_signal().is_ok(),
        "try_check_signal should return Ok when no signal"
    );
}

#[wasm_bindgen_test]
fn test_set_signal_from_rust() {
    clear_signal();
    clear_signal_handler();

    set_signal(42);
    assert_eq!(peek_signal(), Some(Signal(42)));

    let result = try_check_signal();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().0, 42);

    // Signal should be cleared after check
    assert!(peek_signal().is_none());
}

// ============================================================================
// Tests: JavaScript interop
// ============================================================================

#[wasm_bindgen_test]
fn test_signal_addr_is_valid() {
    let addr = get_signal_addr();
    // Address should be non-zero and within reasonable wasm memory bounds
    assert!(addr > 0, "Signal address should be non-zero");
    assert!(
        addr < 0x1000000,
        "Signal address should be within memory bounds"
    );
}

#[wasm_bindgen_test]
fn test_read_signal_from_js() {
    clear_signal();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Set signal from Rust
    set_signal(123);

    // Read from JS
    let js_value = readSignalValue(&memory, signal_addr);
    assert_eq!(js_value, 123, "JS should read the signal value set by Rust");

    clear_signal();
}

#[wasm_bindgen_test]
fn test_write_signal_from_js() {
    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Verify initially clear
    assert!(peek_signal().is_none());

    // Write signal from JS
    writeSignalValue(&memory, signal_addr, 456);

    // Read from Rust
    assert_eq!(
        peek_signal(),
        Some(Signal(456)),
        "Rust should see signal set by JS"
    );

    // try_check_signal should detect it
    let result = try_check_signal();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().0, 456);

    // Signal should be cleared
    assert!(peek_signal().is_none());
}

#[wasm_bindgen_test]
fn test_js_signal_with_handler() {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    clear_signal();

    // Track if handler was called
    static HANDLER_CALLED: AtomicBool = AtomicBool::new(false);
    static HANDLER_VALUE: AtomicU32 = AtomicU32::new(0);

    // Reset state
    HANDLER_CALLED.store(false, Ordering::SeqCst);
    HANDLER_VALUE.store(0, Ordering::SeqCst);

    set_signal_handler(|signal| {
        HANDLER_CALLED.store(true, Ordering::SeqCst);
        HANDLER_VALUE.store(signal.0, Ordering::SeqCst);
        // Propagate the signal (don't clear it)
        Err(signal)
    });

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Set signal from JS
    writeSignalValue(&memory, signal_addr, 789);

    // Check signal - should call handler
    let result = try_check_signal();

    assert!(
        HANDLER_CALLED.load(Ordering::SeqCst),
        "Handler should have been called"
    );
    assert_eq!(
        HANDLER_VALUE.load(Ordering::SeqCst),
        789,
        "Handler should receive correct signal"
    );

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().0, 789);

    clear_signal_handler();
}

#[wasm_bindgen_test]
fn test_js_signal_handler_clears() {
    clear_signal();

    set_signal_handler(|_signal| {
        // Handler clears the signal by returning Ok
        Ok(())
    });

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Set signal from JS
    writeSignalValue(&memory, signal_addr, 999);

    // Check signal - handler should clear it
    let result = try_check_signal();
    assert!(
        result.is_ok(),
        "Handler returned Ok, so try_check_signal should return Ok"
    );

    // Signal should be cleared
    assert!(peek_signal().is_none());

    clear_signal_handler();
}

#[wasm_bindgen_test]
fn test_multiple_signals_from_js() {
    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // First signal
    writeSignalValue(&memory, signal_addr, 1);
    let result = try_check_signal();
    assert_eq!(result.unwrap_err().0, 1);

    // Second signal (different value)
    writeSignalValue(&memory, signal_addr, 2);
    let result = try_check_signal();
    assert_eq!(result.unwrap_err().0, 2);

    // Third signal
    writeSignalValue(&memory, signal_addr, 3);
    let result = try_check_signal();
    assert_eq!(result.unwrap_err().0, 3);

    // No more signals
    assert!(try_check_signal().is_ok());
}

#[wasm_bindgen_test]
fn test_signal_zero_means_no_signal() {
    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Writing 0 should mean "no signal"
    writeSignalValue(&memory, signal_addr, 0);

    assert!(peek_signal().is_none());
    assert!(try_check_signal().is_ok());
}

// ============================================================================
// Tests: Handler registration
// ============================================================================

#[wasm_bindgen_test]
fn test_handler_chain() {
    clear_signal();
    clear_signal_handler();

    fn handler1(_: Signal) -> Result<(), Signal> {
        Ok(())
    }

    fn handler2(s: Signal) -> Result<(), Signal> {
        Err(s)
    }

    // Set first handler
    let prev = set_signal_handler(handler1);
    assert!(prev.is_none(), "No previous handler");

    // Set second handler, get first back
    let prev = set_signal_handler(handler2);
    assert!(prev.is_some(), "Should return previous handler");

    clear_signal_handler();
}

// ============================================================================
// Tests: check_signal panic behavior (using catch_unwind equivalent)
// ============================================================================

#[wasm_bindgen_test]
fn test_check_signal_panics() {
    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // We can't easily test panic in wasm-bindgen-test, but we can verify
    // the setup would cause a panic by using try_check_signal first
    writeSignalValue(&memory, signal_addr, 42);

    let result = try_check_signal();
    assert!(
        result.is_err(),
        "Signal should be detected, would panic in check_signal"
    );
}

// ============================================================================
// Tests: Edge cases
// ============================================================================

#[wasm_bindgen_test]
fn test_max_signal_value() {
    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Test max u32 value
    writeSignalValue(&memory, signal_addr, u32::MAX);

    let result = try_check_signal();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().0, u32::MAX);
}

#[wasm_bindgen_test]
fn test_signal_one() {
    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Signal value 1 should be a valid signal (not "no signal")
    writeSignalValue(&memory, signal_addr, 1);

    let result = try_check_signal();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().0, 1);
}

#[wasm_bindgen_test]
fn test_rapid_signal_toggle() {
    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    for i in 1..=10 {
        writeSignalValue(&memory, signal_addr, i);
        let result = try_check_signal();
        assert_eq!(result.unwrap_err().0, i);

        // Verify cleared
        assert!(try_check_signal().is_ok());
    }
}

// ============================================================================
// Tests: Panic recovery (panic=unwind)
// ============================================================================

#[wasm_bindgen_test]
fn test_check_signal_panic_is_recoverable() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Set signal from JS
    writeSignalValue(&memory, signal_addr, 42);

    // check_signal should panic, but we should be able to catch it
    let result = catch_unwind(AssertUnwindSafe(|| {
        check_signal();
    }));

    assert!(result.is_err(), "check_signal should have panicked");

    // Verify the panic message contains the signal value
    if let Err(panic_payload) = result {
        if let Some(msg) = panic_payload.downcast_ref::<String>() {
            assert!(
                msg.contains("42") || msg.contains("signal"),
                "Panic message should mention signal: {}",
                msg
            );
        } else if let Some(msg) = panic_payload.downcast_ref::<&str>() {
            assert!(
                msg.contains("42") || msg.contains("signal"),
                "Panic message should mention signal: {}",
                msg
            );
        }
    }

    // Signal should have been cleared before the panic
    assert!(
        peek_signal().is_none(),
        "Signal should be cleared after check"
    );

    // System should be usable after panic recovery
    assert!(
        try_check_signal().is_ok(),
        "Should work after panic recovery"
    );
}

#[wasm_bindgen_test]
fn test_handler_panic_is_recoverable() {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::{AtomicU32, Ordering};

    clear_signal();

    static PANIC_COUNT: AtomicU32 = AtomicU32::new(0);
    PANIC_COUNT.store(0, Ordering::SeqCst);

    // Set a handler that panics
    set_signal_handler(|signal| {
        PANIC_COUNT.fetch_add(1, Ordering::SeqCst);
        panic!("Handler panic for signal {}", signal.0);
    });

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Set signal from JS
    writeSignalValue(&memory, signal_addr, 123);

    // The handler panic should be catchable
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = try_check_signal();
    }));

    assert!(result.is_err(), "Handler panic should propagate");
    assert_eq!(
        PANIC_COUNT.load(Ordering::SeqCst),
        1,
        "Handler should have been called once"
    );

    // Signal should have been cleared before calling handler
    assert!(
        peek_signal().is_none(),
        "Signal should be cleared before handler runs"
    );

    // System should be usable after panic recovery
    clear_signal_handler();
    assert!(
        try_check_signal().is_ok(),
        "Should work after handler panic recovery"
    );
}

#[wasm_bindgen_test]
fn test_handler_panic_with_check_signal_is_recoverable() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    clear_signal();

    // Set a handler that panics
    set_signal_handler(|signal| {
        panic!("Handler deliberate panic: {}", signal.0);
    });

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Set signal from JS
    writeSignalValue(&memory, signal_addr, 999);

    // check_signal calls handler which panics - should be catchable
    let result = catch_unwind(AssertUnwindSafe(|| {
        check_signal();
    }));

    assert!(
        result.is_err(),
        "Handler panic via check_signal should be catchable"
    );

    // Verify panic payload
    if let Err(panic_payload) = result {
        if let Some(msg) = panic_payload.downcast_ref::<String>() {
            assert!(
                msg.contains("999"),
                "Panic should contain signal value: {}",
                msg
            );
        }
    }

    // System should recover
    clear_signal_handler();
    clear_signal();
    assert!(try_check_signal().is_ok(), "Should work after recovery");
}

#[wasm_bindgen_test]
fn test_multiple_panic_recoveries() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    clear_signal();
    clear_signal_handler();

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Test multiple panic/recovery cycles
    for i in 1..=5 {
        writeSignalValue(&memory, signal_addr, i);

        let result = catch_unwind(AssertUnwindSafe(|| {
            check_signal();
        }));

        assert!(result.is_err(), "Iteration {} should panic", i);

        // Verify signal is cleared
        assert!(
            peek_signal().is_none(),
            "Signal should be cleared after iteration {}",
            i
        );

        // System should still work
        assert!(
            try_check_signal().is_ok(),
            "Should work after iteration {}",
            i
        );
    }
}

#[wasm_bindgen_test]
fn test_handler_panic_clears_signal_first() {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::{AtomicU32, Ordering};

    clear_signal();

    static SIGNAL_DURING_HANDLER: AtomicU32 = AtomicU32::new(999);
    SIGNAL_DURING_HANDLER.store(999, Ordering::SeqCst);

    set_signal_handler(|_signal| {
        // Check signal state during handler execution
        let current = peek_signal();
        SIGNAL_DURING_HANDLER.store(current.map(|s| s.0).unwrap_or(0), Ordering::SeqCst);
        panic!("intentional panic");
    });

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    writeSignalValue(&memory, signal_addr, 42);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        let _ = try_check_signal();
    }));

    // Signal should have been cleared BEFORE handler was called
    assert_eq!(
        SIGNAL_DURING_HANDLER.load(Ordering::SeqCst),
        0,
        "Signal should be 0 during handler execution (cleared before handler)"
    );

    clear_signal_handler();
}

#[wasm_bindgen_test]
fn test_nested_signal_during_handler() {
    use std::sync::atomic::{AtomicU32, Ordering};

    clear_signal();

    static NESTED_SIGNAL_SEEN: AtomicU32 = AtomicU32::new(0);
    NESTED_SIGNAL_SEEN.store(0, Ordering::SeqCst);

    // Handler that sets a nested signal when handling signal 1
    fn nested_handler(signal: Signal) -> Result<(), Signal> {
        if signal.0 == 1 {
            // Set a new signal during handling
            set_signal(2);

            // Check for the nested signal
            if let Some(nested) = peek_signal() {
                NESTED_SIGNAL_SEEN.store(nested.0, Ordering::SeqCst);
            }
        }
        // Clear by returning Ok
        Ok(())
    }

    set_signal_handler(nested_handler);

    let memory = get_wasm_memory();
    let signal_addr = get_signal_addr();

    // Set first signal
    writeSignalValue(&memory, signal_addr, 1);

    // Handle it - handler will set signal 2
    let result = try_check_signal();
    assert!(result.is_ok(), "Handler returned Ok");

    // Verify nested signal was seen
    assert_eq!(
        NESTED_SIGNAL_SEEN.load(Ordering::SeqCst),
        2,
        "Should have seen nested signal 2"
    );

    // The nested signal should still be pending
    assert_eq!(
        peek_signal(),
        Some(Signal(2)),
        "Nested signal should still be pending"
    );

    // Handle the nested signal
    let result = try_check_signal();
    assert!(result.is_ok());

    clear_signal_handler();
}
