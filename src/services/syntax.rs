use gpui::{Hsla, Rgba};
use std::sync::Arc;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

#[derive(Clone)]
pub struct SyntaxService {
    pub syntax_set: Arc<SyntaxSet>,
    pub theme_set: Arc<ThemeSet>,
}

impl SyntaxService {
    pub fn new() -> Self {
        Self {
            syntax_set: Arc::new(SyntaxSet::load_defaults_newlines()),
            theme_set: Arc::new(ThemeSet::load_defaults()),
        }
    }

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
        let theme = self
            .theme_set
            .themes
            .get(theme_name)
            .or_else(|| self.theme_set.themes.values().next())
            .unwrap();

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
