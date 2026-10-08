//! Thin shim: `cargo run -p on-air-core --example serve` keeps working for the
//! e2e harness until it switches to the `on-air-core` binary.
fn main() {
    on_air_core::run_serve();
}
