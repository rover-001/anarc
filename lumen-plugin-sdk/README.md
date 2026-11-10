# Lumen Plugin SDK

This crate provides Rust-side helpers for plugins compiled as WebAssembly
components. The canonical guest/host contract is [`wit/lumen.wit`](../wit/lumen.wit).
The `component` feature is enabled by default and re-exports `wit_bindgen`; use
its `generate!` macro with the `lumen-plugin` world to create the guest bindings.

The host checks each imported service call against the installed manifest. A
guest must treat denied calls as ordinary errors. The component has no direct
filesystem access; plugin data is available only through the namespaced storage
host calls. Network access is mediated by `http-request` and the manifest's
host allowlist.

Build the SDK's host-side helpers with:

```sh
cargo test --manifest-path lumen-plugin-sdk/Cargo.toml
```

The example component is under `examples/plugins/tidy-tabs/component/`. Building
it for `wasm32-wasip2` requires the Rust target and `cargo-component`; run
`cargo component build --manifest-path examples/plugins/tidy-tabs/component/Cargo.toml`.
