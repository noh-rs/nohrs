//! Print the `config.toml` JSON Schema to stdout.
//!
//! This emits the exact bytes committed to `docs/config.schema.json` (and the
//! same output as `nohrs config schema`), but builds only `nohrs-core`, so it
//! runs on Linux where the gpui-backed `nohrs` binary does not link. CI uses it
//! to keep the committed schema in sync:
//!
//! ```bash
//! cargo run -p nohrs-core --example schema > docs/config.schema.json
//! ```

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", nohrs_core::config::json_schema_string()?);
    Ok(())
}
