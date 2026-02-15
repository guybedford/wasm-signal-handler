//! A signal handler library for WebAssembly applications.
//!
//! This crate provides a mechanism for handling signals in WebAssembly applications,
//! particularly useful for Cloudflare Workers and other wasm-bindgen contexts.
//!
//! # Overview
//!
//! The library exposes a signal variable whose address is exported as a WebAssembly global
//! (`WASM_SIGNAL_ADDR`). External tools (like the Cloudflare Workers runtime) can write to
//! this address to signal the application.
//!
//! # Usage
//!
//! ```rust
//! use wasm_signal_handler::{check_signal, try_check_signal, set_signal_handler, Signal};
//!
//! // Option 1: Check and panic on signal
//! fn process_items(items: &[u32]) {
//!     for item in items {
//!         check_signal(); // Panics if signal received
//!         // process(item);
//!     }
//! }
//!
//! // Option 2: Check and handle gracefully
//! fn process_items_graceful(items: &[u32]) -> Result<(), Signal> {
//!     for item in items {
//!         try_check_signal()?; // Returns Err(Signal) if signal received
//!         // process(item);
//!     }
//!     Ok(())
//! }
//!
//! // Option 3: Register a custom handler
//! fn setup() {
//!     set_signal_handler(|signal| {
//!         // Log the signal, then propagate it
//!         Err(signal)
//!     });
//! }
//! ```

#![no_std]

#[cfg(feature = "std")]
extern crate std;

use core::fmt;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

// ============================================================================
// Signal Type
// ============================================================================

/// Represents a signal value.
///
/// A signal value of `0` means no signal. Any non-zero value represents
/// an active signal, where the value itself is the signal code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signal(pub u32);

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signal({})", self.0)
    }
}

// ============================================================================
// Signal Variable
// ============================================================================

/// The signal variable.
///
/// - `0` means no signal (clear state)
/// - Any non-zero value represents an active signal
///
/// This is an `AtomicU32` to ensure proper memory semantics and prevent
/// compiler optimizations from eliding reads.
#[export_name = "WASM_SIGNAL_ADDR"]
pub static SIGNAL: AtomicU32 = AtomicU32::new(0);

// ============================================================================
// Signal Handler
// ============================================================================

/// A signal handler function.
///
/// The handler receives the signal value and can:
/// - Return `Ok(())` to clear the signal and continue execution
/// - Return `Err(Signal)` to propagate the signal (causes panic in `check_signal`)
///
/// Handlers must be unwind-safe as they may be called during unwinding.
pub type SignalHandler = fn(Signal) -> Result<(), Signal>;

/// Storage for the registered signal handler.
///
/// We store the handler as a raw pointer and transmute on read/write.
/// This is safe because `fn(Signal) -> Result<(), Signal>` is a function pointer
/// with a stable ABI.
static HANDLER: AtomicPtr<()> = AtomicPtr::new(null_mut());

/// Registers a signal handler.
///
/// The handler will be called when `check_signal` or `try_check_signal`
/// detects an active signal. Only one handler can be registered at a time;
/// calling this function replaces any previously registered handler.
///
/// # Returns
///
/// Returns the previously registered handler, if any.
///
/// # Example
///
/// ```rust
/// use wasm_signal_handler::{set_signal_handler, Signal};
///
/// let previous = set_signal_handler(|signal| {
///     // Log the signal, then propagate it
///     Err(signal)
/// });
///
/// // Optionally chain to previous handler
/// if let Some(prev) = previous {
///     // Could call prev(signal) in your handler
/// }
/// ```
pub fn set_signal_handler(handler: SignalHandler) -> Option<SignalHandler> {
    let new_ptr = handler as *mut ();
    let old_ptr = HANDLER.swap(new_ptr, Ordering::SeqCst);

    if old_ptr.is_null() {
        None
    } else {
        // SAFETY: We only store valid SignalHandler function pointers in HANDLER
        Some(unsafe { core::mem::transmute::<*mut (), SignalHandler>(old_ptr) })
    }
}

/// Clears the registered signal handler.
///
/// After calling this, signals will cause `check_signal` to panic and
/// `try_check_signal` to return `Err(Signal)` directly without invoking
/// any handler.
///
/// # Returns
///
/// Returns the previously registered handler, if any.
pub fn clear_signal_handler() -> Option<SignalHandler> {
    let old_ptr = HANDLER.swap(null_mut(), Ordering::SeqCst);

    if old_ptr.is_null() {
        None
    } else {
        // SAFETY: We only store valid SignalHandler function pointers in HANDLER
        Some(unsafe { core::mem::transmute::<*mut (), SignalHandler>(old_ptr) })
    }
}

/// Gets the currently registered signal handler, if any.
pub fn get_signal_handler() -> Option<SignalHandler> {
    let ptr = HANDLER.load(Ordering::SeqCst);

    if ptr.is_null() {
        None
    } else {
        // SAFETY: We only store valid SignalHandler function pointers in HANDLER
        Some(unsafe { core::mem::transmute::<*mut (), SignalHandler>(ptr) })
    }
}

// ============================================================================
// Check Functions
// ============================================================================

/// Handles a detected signal by calling the registered handler.
///
/// This function:
/// 1. Atomically swaps the signal to 0 (clearing it)
/// 2. Calls the registered handler (if any)
/// 3. Returns the handler's result, or `Err(Signal)` if no handler
#[inline]
fn handle_signal(signal_value: u32) -> Result<(), Signal> {
    // Atomically clear the signal and get the value
    // (We already read the value, but swap ensures we clear it)
    SIGNAL.swap(0, Ordering::SeqCst);

    let signal = Signal(signal_value);

    // Check if a handler is registered
    let handler_ptr = HANDLER.load(Ordering::SeqCst);

    if handler_ptr.is_null() {
        // No handler: return error
        Err(signal)
    } else {
        // SAFETY: We only store valid SignalHandler function pointers in HANDLER
        let handler: SignalHandler =
            unsafe { core::mem::transmute::<*mut (), SignalHandler>(handler_ptr) };
        handler(signal)
    }
}

/// Checks for an active signal, returning an error if one is detected.
///
/// This function is designed to be called frequently in hot loops or at
/// entry points to check for pending signals.
///
/// # Returns
///
/// - `Ok(())` if no signal is active, or if the handler cleared the signal
/// - `Err(Signal)` if a signal is active and the handler propagated it
///
/// # Example
///
/// ```rust
/// use wasm_signal_handler::{try_check_signal, Signal};
///
/// fn process_batch(items: &[u32]) -> Result<(), Signal> {
///     for item in items {
///         try_check_signal()?;
///         // process(item);
///     }
///     Ok(())
/// }
/// ```
#[inline]
pub fn try_check_signal() -> Result<(), Signal> {
    let sig = SIGNAL.load(Ordering::Relaxed);

    if sig != 0 {
        handle_signal(sig)
    } else {
        Ok(())
    }
}

/// Checks for an active signal, panicking if one is detected.
///
/// This function is designed to be called frequently in hot loops or at
/// entry points to check for pending signals. If a signal is detected
/// and the handler does not clear it, this function will panic.
///
/// # Panics
///
/// Panics if a signal is detected and:
/// - No handler is registered, or
/// - The registered handler returns `Err(Signal)`
///
/// # Example
///
/// ```rust
/// use wasm_signal_handler::check_signal;
///
/// fn process_batch(items: &[u32]) {
///     for item in items {
///         check_signal(); // Panics if signal received
///         // process(item);
///     }
/// }
/// ```
#[inline]
pub fn check_signal() {
    if let Err(signal) = try_check_signal() {
        panic!("signal received: {}", signal);
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Reads the current signal value without clearing it.
///
/// This is useful for debugging or logging purposes.
///
/// # Returns
///
/// - `None` if no signal is active (value is 0)
/// - `Some(Signal)` if a signal is active
#[inline]
pub fn peek_signal() -> Option<Signal> {
    let sig = SIGNAL.load(Ordering::Relaxed);
    if sig != 0 {
        Some(Signal(sig))
    } else {
        None
    }
}

/// Manually clears the signal without invoking the handler.
///
/// # Returns
///
/// - `None` if no signal was active
/// - `Some(Signal)` with the cleared signal value
#[inline]
pub fn clear_signal() -> Option<Signal> {
    let sig = SIGNAL.swap(0, Ordering::SeqCst);
    if sig != 0 {
        Some(Signal(sig))
    } else {
        None
    }
}

/// Manually sets a signal value.
///
/// This is primarily useful for testing purposes.
///
/// # Arguments
///
/// * `signal` - The signal value to set (0 to clear, non-zero to set)
#[inline]
pub fn set_signal(signal: u32) {
    SIGNAL.store(signal, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_signal() {
        clear_signal();
        assert!(try_check_signal().is_ok());
    }

    #[test]
    fn test_signal_detected() {
        clear_signal_handler();
        set_signal(42);
        let result = try_check_signal();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, 42);
        // Signal should be cleared after check
        assert!(try_check_signal().is_ok());
    }

    #[test]
    fn test_handler_clears_signal() {
        set_signal_handler(|_signal| Ok(()));
        set_signal(1);
        assert!(try_check_signal().is_ok());
        clear_signal_handler();
    }

    #[test]
    fn test_handler_propagates_signal() {
        set_signal_handler(|signal| Err(Signal(signal.0 * 2)));
        set_signal(21);
        let result = try_check_signal();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, 42);
        clear_signal_handler();
    }

    #[test]
    fn test_set_handler_returns_previous() {
        clear_signal_handler();

        fn handler1(_: Signal) -> Result<(), Signal> {
            Ok(())
        }
        fn handler2(_: Signal) -> Result<(), Signal> {
            Err(Signal(0))
        }

        assert!(set_signal_handler(handler1).is_none());
        let prev = set_signal_handler(handler2);
        assert!(prev.is_some());

        clear_signal_handler();
    }

    #[test]
    fn test_peek_signal() {
        clear_signal();
        assert!(peek_signal().is_none());

        set_signal(123);
        assert_eq!(peek_signal(), Some(Signal(123)));

        // Peek doesn't clear
        assert_eq!(peek_signal(), Some(Signal(123)));

        clear_signal();
    }
}
