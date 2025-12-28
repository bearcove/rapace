+++
title = "Future directions"
description = "Ideas rapace might enable later"
+++

This page lists features and directions that are not yet implemented but are interesting to explore.

## Plugin reloads

Today, dodeca starts its plugins once at launch and they run until the process exits. Because plugins are separate executables that talk to the host over rapace instead of being dynamically loaded into the same address space, it is at least conceptually possible to do more:

- rebuild a plugin binary;
- stop the old process;
- start the new one and reconnect over the same RPC surface.

Getting that right involves details that are not implemented today (how to hand off in‑flight requests, how to coordinate state between old and new instances, etc.), but the "host + external plugin over IPC/RPC" design makes the problem more tractable than shared‑library hot‑reloading inside a single process.

## Mutation at a distance

Another idea is to treat rapace as a way to mutate state at a distance in a controlled way. Instead of always sending full values back and forth, you could imagine:

- keeping some piece of state on one side (a host, a plugin, or a remote service);
- describing changes as diffs or small operations;
- applying those diffs over a rapace channel, possibly across different transports (SHM, WebSocket, stream).

The existing pieces (service traits, facet‑based schemas, postcard encoding, frames, channels) are all oriented around sending typed messages. A future layer could interpret some of those messages as patches or transactional updates to long‑lived objects, whether they live in the same machine or on the other end of a network connection.

## Server generation for non-Rust languages

The current code generators produce **client** bindings for Swift and TypeScript. Server-side bindings (handling incoming requests, dispatching to implementations) are not yet generated for non-Rust languages.

For many use cases this is fine — Rust is the natural choice for high-performance server implementations. But there are scenarios where you might want:

- A TypeScript server running in Node.js or Deno
- A Swift server for iOS/macOS services that accept connections from Rust clients

The pieces are in place (the registry has all type shapes, the spec defines the protocol), but the generators don't yet emit server scaffolding.

## Bidirectional streaming

The protocol supports bidirectional STREAM channels (both sides can send items), but the Rust implementation and code generators don't fully expose this yet. The spec defines it; implementing it is a matter of wiring up the APIs.

## Go and Java implementations

The [Language Mappings](/spec/language-mappings/) spec defines how types map to Go and Java, but no code generators exist yet. The TypeScript and Swift generators provide a template for how to build them.
