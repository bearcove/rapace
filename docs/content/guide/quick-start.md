+++
title = "Quick Start"
description = "Get started with Rapace in minutes"
+++

# Quick Start

Get up and running with Rapace in just a few minutes.

## Installation

Add Rapace to your `Cargo.toml`:

```toml
[dependencies]
rapace = "0.1"
rapace-transport-mem = "0.1"  # For in-memory transport
tokio = { version = "1.0", features = ["full"] }
```

## Define a Service

Use the `#[rapace::service]` macro to define your RPC interface:

```rust
use rapace::service;

#[rapace::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn multiply(&self, a: i32, b: i32) -> i32;
}
```

## Implement the Service

```rust
struct MyCalculator;

impl Calculator for MyCalculator {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
    
    async fn multiply(&self, a: i32, b: i32) -> i32 {
        a * b
    }
}
```

## Create Client and Server

```rust
use rapace::RpcSession;
use rapace_transport_mem::MemTransport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a transport pair
    let (client_transport, server_transport) = MemTransport::pair();
    
    // Start the server
    let service = MyCalculator;
    let server = CalculatorServer::new(service);
    tokio::spawn(async move {
        server.serve(server_transport).await
    });
    
    // Create a client
    let session = RpcSession::new(client_transport);
    let client = CalculatorClient::new(session);
    
    // Make RPC calls
    let result = client.add(5, 3).await?;
    println!("5 + 3 = {}", result);
    
    Ok(())
}
```

That's it! You now have a working RPC service with Rapace.