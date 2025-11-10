# Nohrs

Develop a fast, flexible, and extensible file explorer using Rust and gpui as a modern alternative to macOS Finder. The goal is to achieve both everyday usability and power-user functionality through a seamless, high-performance interface.

## Development

- Toolchain: Rust (stable), pinned via `rust-toolchain.toml`.
- Build (core library only): `cargo build`
- Build GUI binary (placeholder UI): `cargo build --features gui`
  - Run GUI binary: `cargo run --features gui --bin nohr`

Notes

- The GUI is currently a placeholder entry-point that will be wired to gpui once a pinned version is selected.

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

# Planned Features

### Navigation & UI
- Tabs and split view for parallel directories  
- Inline preview for images, PDFs, text, and Markdown  
- Command palette for quick action search (VS Code-style)  
- File icons and custom emoji labels  

### File Operations
- In-place editing for `.txt` and `.md`  
- Bulk rename with regex and metadata rules  
- Advanced drag-and-drop (S3 upload, Git staging)  
- Clipboard history for multiple copied items  

### Search & Indexing
- Fast full-text search with Tantivy + ripgrep (fuzzy supported)  
- Smart folders filtered by tags, type, or date  
- Search inside previews (PDF, Markdown, code)  
- File ranking by open frequency, recency, and relevance etc.

### Terminal Integration
- Built-in PTY linked to current directory  
- Drag-to-escape paste for file paths  
- Task runner for one-click command execution  

### Git Integration
- Sidebar for status and branches  
- Diff preview and blame view  
- Merge-conflict resolution UI  

### S3-Compatible Storage
- MinIO, Wasabi, and Cloudflare R2 support  
- Transfer queue and parallel uploads  
- Metadata editing and presigned URLs  
- Offline cache and sync recovery  

### Automation & Extensions
- Plugin system for custom UI or features  
- Folder-watch actions (auto-tag, auto-transfer)  
- CLI / HTTP API for external control  
- Remote browsing via SSH  

## Contributing

Contributions are welcome! Please feel free to submit a pull request.

### Code Style

- Rust (stable; toolchain pinned via rust-toolchain.toml): Follow standard conventions
