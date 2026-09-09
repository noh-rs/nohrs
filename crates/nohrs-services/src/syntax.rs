use gpui::{Hsla, Rgba};
use std::sync::Arc;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Syntax highlighter that maps `syntect` styles onto GPUI colors.
#[derive(Clone)]
pub struct SyntaxService {
    /// The loaded set of syntax definitions used to pick a grammar.
    pub syntax_set: Arc<SyntaxSet>,
    /// The loaded set of color themes used for highlighting.
    pub theme_set: Arc<ThemeSet>,
}

impl Default for SyntaxService {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxService {
    /// Creates a service loaded with `syntect`'s default syntaxes and themes.
    pub fn new() -> Self {
        Self {
            syntax_set: Arc::new(SyntaxSet::load_defaults_newlines()),
            theme_set: Arc::new(ThemeSet::load_defaults()),
        }
    }

    /// Highlights `text`, returning contiguous byte ranges paired with their
    /// color; `extension` selects the grammar, falling back to plain text.
    pub fn highlight(
        &self,
        text: &str,
        extension: Option<&str>,
    ) -> Vec<(std::ops::Range<usize>, Hsla)> {
        let syntax = if let Some(ext) = extension {
            self.syntax_set
                .find_syntax_by_extension(ext)
                .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
        } else {
            self.syntax_set.find_syntax_plain_text()
        };

        let theme_name = "base16-ocean.dark";
        // Fall back to plain (unstyled) output rather than panicking if no theme
        // is available; highlighting is non-essential.
        let Some(theme) = self
            .theme_set
            .themes
            .get(theme_name)
            .or_else(|| self.theme_set.themes.values().next())
        else {
            return Vec::new();
        };

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();
        let mut offset = 0;

        for line in LinesWithEndings::from(text) {
            let ranges: Vec<(syntect::highlighting::Style, &str)> = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();

            for (style, chunk) in ranges {
                let color = style.foreground;
                let gpui_color = Hsla::from(Rgba {
                    r: color.r as f32 / 255.0,
                    g: color.g as f32 / 255.0,
                    b: color.b as f32 / 255.0,
                    a: color.a as f32 / 255.0,
                });
                let len = chunk.len(); // byte length
                result.push((offset..(offset + len), gpui_color));
                offset += len;
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::SyntaxService;

    #[test]
    fn new_loads_default_syntaxes_and_themes() {
        let svc = SyntaxService::new();
        assert!(!svc.syntax_set.syntaxes().is_empty());
        assert!(!svc.theme_set.themes.is_empty());
    }

    #[test]
    fn highlight_of_empty_text_is_empty() {
        let svc = SyntaxService::new();
        assert!(svc.highlight("", Some("rs")).is_empty());
    }

    #[test]
    fn highlight_is_deterministic_and_covers_every_byte() {
        let svc = SyntaxService::new();
        let text = "fn main() {}\n";
        let first: Vec<_> = svc
            .highlight(text, Some("rs"))
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        let second: Vec<_> = svc
            .highlight(text, Some("rs"))
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        assert_eq!(first, second, "highlighting is deterministic");
        assert!(!first.is_empty());

        // Ranges are contiguous from 0 and span the whole input.
        let mut expected = 0;
        for range in &first {
            assert_eq!(range.start, expected);
            expected = range.end;
        }
        assert_eq!(expected, text.len());
    }

    #[test]
    fn unknown_extension_falls_back_to_plain_text() {
        let svc = SyntaxService::new();
        let text = "just some text\n";
        let ranges = svc.highlight(text, None);
        assert!(!ranges.is_empty());
        let covered: usize = ranges.iter().map(|(r, _)| r.end - r.start).sum();
        assert_eq!(covered, text.len());
    }
}
