use super::view::preview::editor::PreviewEditor;
use super::ExplorerPage;
use gpui::{AppContext, Context, Entity, Window};

impl ExplorerPage {
    pub(super) fn update_editor_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor_entity) = &self.preview_editor {
            let query = self.search_query.clone();
            let editor_entity = editor_entity.clone();
            editor_entity.update(cx, |editor, cx| {
                editor.set_search_query(query, window, cx);
            });
        }
    }

    pub(super) fn scroll_to_line(
        &mut self,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = &self.preview_text {
            let mut current_off = 0;
            let mut found_offset = None;
            let target_idx = line.saturating_sub(1);

            for (i, line_str) in text.lines().enumerate() {
                if i == target_idx {
                    found_offset = Some(current_off);
                    break;
                }
                let consumed = line_str.len();
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

            if let Some(offset) = found_offset {
                if let Some(editor) = &self.preview_editor {
                    let editor = editor.clone();
                    editor.update(cx, |editor, cx| {
                        editor.scroll_to(offset, window, cx);
                    });
                }
            }
        }
    }

    pub(super) fn open_preview(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_editor = None;
        self.preview_image_path = None;
        self.preview_message = None;
        self.preview_text = None;
        self.preview_path = None;

        if let Ok(md) = std::fs::metadata(&path) {
            if md.is_file() {
                if md.len() > 1024 * 1024 * 2 {
                    self.preview_path = Some(path);
                    self.preview_message = Some("(File too large to preview)".to_string());
                    return;
                }

                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(text) = String::from_utf8(bytes) {
                        self.open_text_preview(path, text, window, cx);
                        return;
                    } else {
                        if self.try_open_image_preview(&path) {
                            return;
                        }
                    }
                }
            }
        }

        self.preview_path = Some(path);
        self.preview_message = Some("(Preview not available for this file)".to_string());
    }

    fn open_text_preview(
        &mut self,
        path: String,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_path = Some(path.clone());
        self.preview_text = Some(text.clone());

        let editor_view: Entity<PreviewEditor> = cx.new(|cx| PreviewEditor::new(window, cx));
        let extension = std::path::Path::new(&path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let language = match extension.as_str() {
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
        .to_string();

        editor_view.update(cx, |editor, cx| {
            editor.set_text(text.clone(), window, cx);
            if language != "plain" {
                editor.set_language(language, window, cx);
            }
        });

        self.preview_editor = Some(editor_view.into());
        self.update_editor_search(window, cx);
        self.scroll_to_first_match(&path, &text, window, cx);
    }

    fn scroll_to_first_match(
        &mut self,
        path: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(results) = &self.search_results else {
            return;
        };
        let Some(file_result) = results.iter().find(|r| r.path == path) else {
            return;
        };
        let Some(first_match) = file_result.matches.first() else {
            return;
        };

        let mut current_off = 0;
        let target_line = first_match.line_number;
        let mut found_offset = None;

        for (i, line) in text.lines().enumerate() {
            if i == target_line {
                found_offset = Some(current_off);
                break;
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

        if let Some(offset) = found_offset {
            if let Some(editor) = &self.preview_editor {
                let editor = editor.clone();
                editor.update(cx, |editor, cx| {
                    editor.scroll_to(offset, window, cx);
                });
            }
        }
    }

    fn try_open_image_preview(&mut self, path: &str) -> bool {
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" => {
                self.preview_path = Some(path.to_string());
                self.preview_image_path = Some(path.to_string());
                true
            }
            _ => false,
        }
    }
}
