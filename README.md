# wasm-signal-handler

A signal handler library for WebAssembly applications, designed for use with wasm-bindgen and runtimes like Cloudflare Workers.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
wasm-signal-handler = "0.1"
```

## Quick Start

```rust
use wasm_signal_handler::{check_signal, try_check_signal, Signal};

// In CPU-intensive loops, check for signals periodically
fn process_items(items: &[Item]) -> Result<(), Signal> {
    for item in items {
        try_check_signal()?;  // Returns Err(Signal) if interrupted
        process(item);
    }
    Ok(())
}
```

## Usage

### Checking for Signals

The library provides two functions for checking if a signal has been received:

#### `try_check_signal()` - Recommended

Returns `Result<(), Signal>`. Use this when you want to handle interruption gracefully:

```rust
use wasm_signal_handler::{try_check_signal, Signal};

fn compute_heavy(data: &[u8]) -> Result<Vec<u8>, Signal> {
    let mut result = Vec::new();
    
    for chunk in data.chunks(1024) {
        try_check_signal()?;  // Early return on signal
        result.extend(process_chunk(chunk));
    }
    
    Ok(result)
}
```

#### `check_signal()` - Panic on Signal

Panics if a signal is detected. Use this when you want automatic unwinding:

```rust
use wasm_signal_handler::check_signal;

fn process_batch(items: &[Item]) {
    for item in items {
        check_signal();  // Panics if signal received
        process(item);
    }
}
```

### Where to Place Signal Checks

Add signal checks at strategic points in your code:

1. **Loop iterations** - Check at the start of each iteration in long-running loops
2. **Recursive calls** - Check before recursive function calls
3. **Entry points** - Check at the beginning of request handlers
4. **Between stages** - Check between distinct processing phases

```rust
fn handle_request(req: Request) -> Result<Response, Signal> {
    try_check_signal()?;  // Entry point check
    
    let data = parse_request(req)?;
    try_check_signal()?;  // Between stages
    
    let result = process_data(&data)?;
    try_check_signal()?;
    
    Ok(build_response(result))
}

fn process_data(data: &Data) -> Result<Output, Signal> {
    for item in &data.items {
        try_check_signal()?;  // Loop check
        // ... processing
    }
    Ok(output)
}
```

### Registering a Signal Handler

You can register a custom handler that runs when a signal is detected:

```rust
use wasm_signal_handler::{set_signal_handler, Signal};

fn setup() {
    set_signal_handler(|signal| {
        // Log the signal
        log::warn!("Received signal: {}", signal.0);
        
        // Return Ok(()) to clear the signal and continue
        // Return Err(signal) to propagate the error
        Err(signal)
    });
}
```

#### Handler Return Values

- `Ok(())` - Clears the signal; execution continues normally
- `Err(Signal)` - Propagates the signal; `try_check_signal` returns this error, `check_signal` panics

#### Handler Use Cases

**Logging and metrics:**
```rust
set_signal_handler(|signal| {
    metrics::increment("signals_received");
    log::info!("Signal {} received", signal.0);
    Err(signal)  // Still propagate
});
```

**Cleanup before termination:**
```rust
set_signal_handler(|signal| {
    cleanup_resources();
    flush_buffers();
    Err(signal)
});
```

**Conditional handling:**
```rust
set_signal_handler(|signal| {
    if can_ignore_signal() {
        Ok(())  // Clear and continue
    } else {
        Err(signal)  // Propagate
    }
});
```

### Handler Management

```rust
use wasm_signal_handler::{
    set_signal_handler, 
    clear_signal_handler, 
    get_signal_handler
};

// Set a handler, get the previous one back
let previous = set_signal_handler(my_handler);

// Chain handlers
set_signal_handler(|signal| {
    // Do something first
    log::info!("Signal: {}", signal.0);
    
    // Then call the previous handler
    if let Some(prev) = previous {
        prev(signal)
    } else {
        Err(signal)
    }
});

// Remove the handler
clear_signal_handler();

// Check if a handler is registered
if get_signal_handler().is_some() {
    // Handler is registered
}
```

### Utility Functions

```rust
use wasm_signal_handler::{peek_signal, clear_signal, set_signal};

// Check signal without clearing it
if let Some(signal) = peek_signal() {
    log::warn!("Signal pending: {}", signal.0);
}

// Manually clear the signal
if let Some(signal) = clear_signal() {
    log::info!("Cleared signal: {}", signal.0);
}

// Set a signal (useful for testing)
set_signal(42);
```

## Error Handling Patterns

### With `?` Operator

```rust
fn process() -> Result<Output, Signal> {
    try_check_signal()?;
    // ...
    Ok(output)
}
```

### Converting to Custom Error

```rust
#[derive(Debug)]
enum MyError {
    Interrupted(Signal),
    Other(String),
}

impl From<Signal> for MyError {
    fn from(signal: Signal) -> Self {
        MyError::Interrupted(signal)
    }
}

fn process() -> Result<Output, MyError> {
    try_check_signal()?;  // Automatically converts
    Ok(output)
}
```

### With `catch_unwind` (panic=unwind only)

```rust
use std::panic::{catch_unwind, AssertUnwindSafe};

let result = catch_unwind(AssertUnwindSafe(|| {
    check_signal();
    do_work()
}));

match result {
    Ok(value) => println!("Success: {:?}", value),
    Err(_) => println!("Interrupted or panicked"),
}
```

## Signal Values

- `0` = No signal (clear state)
- Non-zero = Active signal (the value is the signal code)

The specific meaning of non-zero values depends on the host runtime.

---

## For Runtime Implementers

This section describes how host runtimes (like Cloudflare Workers) can trigger signals.

### Signal Address Export

The library exports a WebAssembly global named `WASM_SIGNAL_ADDR` containing the memory address of the signal variable:

```rust
#[no_mangle]
pub static WASM_SIGNAL_ADDR: &AtomicU32 = &SIGNAL;
```

### Reading the Signal Address

From JavaScript/host code:

```javascript
// Get the exported global
const signalAddr = instance.exports.WASM_SIGNAL_ADDR.value;

// This is the memory address where the signal u32 is stored
console.log("Signal address:", signalAddr);
```

### Writing a Signal

To trigger a signal, write a non-zero u32 to the signal address:

```javascript
const memory = instance.exports.memory;
const view = new DataView(memory.buffer);
const signalAddr = instance.exports.WASM_SIGNAL_ADDR.value;

// Set signal (e.g., signal code 1 for termination)
view.setUint32(signalAddr, 1, true);  // true = little-endian
```

### Clearing a Signal

Write `0` to clear:

```javascript
view.setUint32(signalAddr, 0, true);
```

### Cloudflare Workers Integration

Cloudflare Workers can use this mechanism to signal Wasm modules for:

- **CPU time limits** - Signal when approaching the CPU time limit
- **Memory pressure** - Signal when memory usage is high
- **Graceful shutdown** - Signal to allow cleanup before termination

The runtime writes to the signal address, and properly instrumented Rust code will detect and handle the signal at the next `check_signal()` or `try_check_signal()` call.

### Memory Layout

```
WASM_SIGNAL_ADDR (global i32) --> points to --> SIGNAL (u32 in linear memory)
                                                 │
                                                 ├── 0x00000000 = no signal
                                                 └── 0x00000001+ = signal code
```

## Building with Panic Unwind Support

For full panic recovery support in WebAssembly, build with nightly and exception handling:

```toml
# rust-toolchain.toml
[toolchain]
channel = "nightly"
targets = ["wasm32-unknown-unknown"]
```

```toml
# .cargo/config.toml
[target.wasm32-unknown-unknown]
rustflags = ["-Cpanic=unwind", "-Ctarget-feature=+exception-handling"]

[unstable]
build-std = ["std", "panic_unwind"]
```

## License

MIT OR Apache-2.0
