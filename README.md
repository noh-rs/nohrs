# nohr

Develop a fast, flexible, and extensible file explorer using Rust and gpui as a modern alternative to macOS Finder. The goal is to achieve both everyday usability and power-user functionality through a seamless, high-performance interface.

## Development

- Toolchain: Rust (stable), pinned via `rust-toolchain.toml`.
- Build (core library only): `cargo build`
- Build GUI binary (placeholder UI): `cargo build --features gui`
  - Run GUI binary: `cargo run --features gui --bin nohr`

Notes

- The GUI is currently a placeholder entry-point that will be wired to gpui once a pinned version is selected.
- HTTP endpoints are scaffolded but not implemented; focus is GUI-first for US1.

### macOS prerequisites for gpui

gpui uses Metal on macOS and requires Xcode and the Metal toolchain.

1. Install Xcode from the App Store (launch it once to finish setup)
2. Install command line tools:
   - `xcode-select --install`
3. Ensure CLI uses the installed Xcode:
   - `sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer`
4. If build complains about missing Metal toolchain, fetch it:
   - `xcodebuild -downloadComponent MetalToolchain`

After completing the above, try building the GUI again.

## Contributing

Contributions are welcome! Please feel free to submit a pull request.

### Code Style

- Rust (stable; toolchain pinned via rust-toolchain.toml): Follow standard conventions

### Documentation

- All documentation is written in Japanese.
