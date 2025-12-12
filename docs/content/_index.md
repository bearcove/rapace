+++
title = "Rapace"
description = "A high-performance RPC framework for Rust"
+++

A high-performance RPC framework for Rust with support for shared memory, TCP, WebSocket, and in-process transports.

**Multiple transports.** Choose the right transport for your use case: shared memory for ultra-low latency, TCP for network communication, WebSocket for browsers, or in-memory for testing.

**Type-safe streaming.** Full support for server and client streaming with compile-time verification of RPC calls.

**Code generation.** Write your service interface once with `#[rapace::service]` and get client and server implementations automatically.

```rust
use rapace::service;

#[rapace::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn stream_numbers(&self, start: i32, count: i32) -> impl Stream<Item = i32>;
}
```

**Cross-platform.** Works on Linux, macOS, Windows, and WebAssembly.