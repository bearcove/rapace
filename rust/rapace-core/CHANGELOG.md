# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0](https://github.com/bearcove/rapace/compare/rapace-core-v0.5.0...rapace-core-v0.6.0) - 2026-01-11

### Fixed

- use enumerate() for varint loop counter (clippy)

### Other

- Update for facet variance API change
- Remove shm reference for wasm and windows target ([#148](https://github.com/bearcove/rapace/pull/148))
- harden stream framing
- move non-normative guidance out of spec
- drop canonical encoding; remove compliance doc
- Add spec impl annotations to improve tracey coverage
- Reset conformance tests to single working template ([#142](https://github.com/bearcove/rapace/pull/142))
- Contain unsafe code in dedicated locations ([#140](https://github.com/bearcove/rapace/pull/140))
- Gut conformance tests for fresh rewrite ([#138](https://github.com/bearcove/rapace/pull/138))
- Upgrade facet dependencies from git ([#134](https://github.com/bearcove/rapace/pull/134))
- Add 45 conformance tests for core protocol rules ([#133](https://github.com/bearcove/rapace/pull/133))
- Split spec coverage CI into per-language jobs, use tracey ([#130](https://github.com/bearcove/rapace/pull/130))
- Improve spec coverage tracking and add conformance tests ([#129](https://github.com/bearcove/rapace/pull/129))
- Add rapace-protocol crate and spec conformance tooling ([#128](https://github.com/bearcove/rapace/pull/128))
- Add spec references to Rust implementation, refresh docs ([#127](https://github.com/bearcove/rapace/pull/127))
- Replace serde/serde_json with facet-json/facet-value
- Remove futures-macro dependency by using select() function
- Replace futures meta-crate with futures-util + futures-core
- Remove pair SHM transport in favor of hub-only architecture ([#124](https://github.com/bearcove/rapace/pull/124))
- Spec work
- Consolidate into multi-language monorepo ([#120](https://github.com/bearcove/rapace/pull/120))

## [0.4.0](https://github.com/bearcove/rapace/compare/rapace-core-v0.3.0...rapace-core-v0.4.0) - 2025-12-14

### Other

- Fix browser tests
- update SHM doorbell/hub notes ([#38](https://github.com/bearcove/rapace/pull/38))
