use nohrs_core::config;

use gpui::{AppContext, AsyncWindowContext, Context, Window};

use super::ExplorerPage;
use super::view::preview::editor::PreviewEditor;

/// Result of reading a file for preview off the UI thread.
enum PreviewOutcome {
    TooLarge,
    Text(String),
    Image,
    Unsupported,
}

fn detect_language(path: &str) -> String {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "rs" => "rust",
        "md" => "markdown",
        "json" => "json",
        "js" => "javascript",
        "ts" => "typescript",
        "html" => "html",
        "go" => "go",
        "zig" => "zig",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "css" => "css",
        "c" => "c",
        "cpp" => "cpp",
        _ => "plain",
    }
    .to_string()
}

fn is_image_path(path: &str) -> bool {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp"
    )
}

/// Raster image formats that cannot be meaningfully decoded as UTF-8 text.
/// SVG is intentionally excluded so it still previews as text in the editor.
fn is_binary_image_path(path: &str) -> bool {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
    )
}

/// Reads `path` and classifies it for preview. Runs on a background thread, so
/// it must not touch any GPUI state.
// Always invoked from `cx.background_spawn` (see `open_preview`), so its
// blocking reads run off the GPUI foreground thread.
#[allow(clippy::disallowed_methods)]
fn read_preview(path: &str) -> PreviewOutcome {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return PreviewOutcome::Unsupported,
    };
    if !metadata.is_file() {
        return PreviewOutcome::Unsupported;
    }
    if metadata.len() > config::PREVIEW_MAX_FILE_SIZE {
        return PreviewOutcome::TooLarge;
    }
    // Binary images are rendered from disk by path, so avoid reading the whole
    // file and attempting a UTF-8 decode just to discover it is binary.
    if is_binary_image_path(path) {
        return PreviewOutcome::Image;
    }
    match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => PreviewOutcome::Text(text),
            Err(_) if is_image_path(path) => PreviewOutcome::Image,
            Err(_) => PreviewOutcome::Unsupported,
        },
        Err(_) => PreviewOutcome::Unsupported,
    }
}

/// Computes the byte offset of the start of the given 0-based line index,
/// accounting for `\n` and `\r\n` line endings.
fn line_start_offset(text: &str, target_line: usize) -> Option<usize> {
    let mut current_off = 0;
    for (i, line) in text.lines().enumerate() {
        if i == target_line {
            return Some(current_off);
        }
        let consumed = line.len();
        let remainder = &text[current_off + consumed..];
        let newline_len = if remainder.starts_with("\r\n") {
            2
        } else if remainder.starts_with('\n') {
            1
        } else {
            0
        };
        current_off += consumed + newline_len;
    }
    None
}

impl ExplorerPage {
    pub(crate) fn open_preview(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_editor = None;
        self.preview_image_path = None;
        self.preview_message = None;
        self.preview_text = None;
        // Record the path being loaded so that out-of-order async completions
        // (the user clicking another file before this read finishes) can be
        // detected and discarded below.
        self.preview_path = Some(path.clone());
        cx.notify();

        let read_task = cx.background_spawn({
            let path = path.clone();
            async move { read_preview(&path) }
        });

        cx.spawn_in(
            window,
            move |this: gpui::WeakEntity<ExplorerPage>, cx: &mut AsyncWindowContext| {
                let mut cx = cx.clone();
                async move {
                    let outcome = read_task.await;
                    if let Err(error) = this.update_in(&mut cx, |this, window, cx| {
                        // Skip if a newer preview was requested while we were reading.
                        if this.preview_path.as_deref() != Some(path.as_str()) {
                            return;
                        }
                        this.apply_preview_outcome(path, outcome, window, cx);
                    }) {
                        tracing::debug!("Preview update skipped: {error}");
                    }
                }
            },
        )
        .detach();
    }

    fn apply_preview_outcome(
        &mut self,
        path: String,
        outcome: PreviewOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            PreviewOutcome::TooLarge => {
                self.preview_path = Some(path);
                self.preview_message = Some("(File too large to preview)".to_string());
            }
            PreviewOutcome::Image => {
                self.preview_path = Some(path.clone());
                self.preview_image_path = Some(path);
            }
            PreviewOutcome::Unsupported => {
                self.preview_path = Some(path);
                self.preview_message = Some("(Preview not available for this file)".to_string());
            }
            PreviewOutcome::Text(text) => {
                self.preview_path = Some(path.clone());
                self.preview_text = Some(text.clone());

                let editor_view = cx.new(|cx| PreviewEditor::new(window, cx));
                let language = detect_language(&path);
                editor_view.update(cx, |editor, cx| {
                    editor.set_text(text.clone(), window, cx);
                    if language != "plain" {
                        editor.set_language(language, window, cx);
                    }
                });
                self.preview_editor = Some(editor_view);

                // Highlights (search only; syntax handled by editor)
                self.update_editor_search(window, cx);

                // Scroll to the first match for the active query, if any.
                if let Some(results) = &self.search_results {
                    if let Some(file_result) = results.iter().find(|r| r.path == path) {
                        if let Some(first_match) = file_result.matches.first() {
                            // `line_number` is 1-based; `line_start_offset` takes a 0-based index.
                            if let Some(offset) =
                                line_start_offset(&text, first_match.line_number.saturating_sub(1))
                            {
                                if let Some(editor) = self.preview_editor.clone() {
                                    editor.update(cx, |editor, cx| {
                                        editor.scroll_to(offset, window, cx);
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn update_editor_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor_entity) = self.preview_editor.clone() {
            let query = self.search_query.clone();
            editor_entity.update(cx, |editor, cx| {
                editor.set_search_query(query, window, cx);
            });
        }
    }

    pub(crate) fn scroll_to_line(
        &mut self,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.preview_text.clone() else {
            return;
        };
        // 1-based line number to 0-based index
        let target_idx = line.saturating_sub(1);
        if let Some(offset) = line_start_offset(&text, target_idx) {
            if let Some(editor) = self.preview_editor.clone() {
                editor.update(cx, |editor, cx| {
                    editor.scroll_to(offset, window, cx);
                });
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn detect_language_maps_known_extensions() {
        for (path, expected) in [
            ("a.rs", "rust"),
            ("a.md", "markdown"),
            ("a.json", "json"),
            ("a.js", "javascript"),
            ("a.ts", "typescript"),
            ("a.html", "html"),
            ("a.go", "go"),
            ("a.zig", "zig"),
            ("a.toml", "toml"),
            ("a.yaml", "yaml"),
            ("a.yml", "yaml"),
            ("a.css", "css"),
            ("a.c", "c"),
            ("a.cpp", "cpp"),
        ] {
            assert_eq!(detect_language(path), expected, "for {path}");
        }
    }

    #[test]
    fn detect_language_is_case_insensitive_and_defaults_to_plain() {
        assert_eq!(detect_language("MAIN.RS"), "rust");
        assert_eq!(detect_language("notes.unknown"), "plain");
        assert_eq!(detect_language("noext"), "plain");
    }

    #[test]
    fn image_path_classification() {
        for ext in ["png", "jpg", "jpeg", "gif", "bmp", "svg", "webp"] {
            assert!(is_image_path(&format!("a.{ext}")), "{ext} is an image");
        }
        assert!(is_image_path("PHOTO.PNG"));
        assert!(!is_image_path("a.txt"));
    }

    #[test]
    fn binary_image_excludes_svg() {
        assert!(is_binary_image_path("a.png"));
        assert!(is_binary_image_path("a.gif"));
        // SVG is text, so it is not treated as a binary image.
        assert!(!is_binary_image_path("a.svg"));
        assert!(!is_binary_image_path("a.txt"));
    }

    #[test]
    fn read_preview_classifies_files() {
        let dir = tempfile::tempdir().unwrap();

        let text_path = dir.path().join("note.txt");
        std::fs::write(&text_path, "hello\nworld").unwrap();
        match read_preview(&text_path.to_string_lossy()) {
            PreviewOutcome::Text(body) => assert_eq!(body, "hello\nworld"),
            _ => panic!("expected Text"),
        }

        // Binary-image extension short-circuits to Image without decoding.
        let png_path = dir.path().join("pic.png");
        std::fs::write(&png_path, [0u8, 1, 2, 3]).unwrap();
        assert!(matches!(
            read_preview(&png_path.to_string_lossy()),
            PreviewOutcome::Image
        ));

        // Non-UTF-8, non-image → Unsupported.
        let bin_path = dir.path().join("blob.bin");
        std::fs::write(&bin_path, [0xff, 0xfe, 0xfd]).unwrap();
        assert!(matches!(
            read_preview(&bin_path.to_string_lossy()),
            PreviewOutcome::Unsupported
        ));

        // A directory and a missing path are both Unsupported.
        assert!(matches!(
            read_preview(&dir.path().to_string_lossy()),
            PreviewOutcome::Unsupported
        ));
        assert!(matches!(
            read_preview("/nonexistent/path/here"),
            PreviewOutcome::Unsupported
        ));
    }

    #[test]
    fn line_start_offset_handles_lf_crlf_and_bounds() {
        let lf = "a\nbb\nccc";
        assert_eq!(line_start_offset(lf, 0), Some(0));
        assert_eq!(line_start_offset(lf, 1), Some(2));
        assert_eq!(line_start_offset(lf, 2), Some(5));
        assert_eq!(line_start_offset(lf, 3), None);

        let crlf = "a\r\nbb";
        assert_eq!(line_start_offset(crlf, 1), Some(3));
    }
}
