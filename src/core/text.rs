use std::ops::Range;

pub fn truncate_middle(text: &str, max_len: usize) -> String {
    let char_count = text.chars().count();

    if char_count <= max_len {
        return text.to_string();
    }

    if max_len < 4 {
        // Not enough room for "x..." pattern
        return text.chars().take(max_len).collect();
    }

    if let Some(dot_pos) = text.rfind('.') {
        let name_part = &text[..dot_pos];
        let ext_part = &text[dot_pos..];
        let name_chars: Vec<char> = name_part.chars().collect();
        let ext_chars = ext_part.chars().count();

        let available = max_len.saturating_sub(ext_chars).saturating_sub(3);
        if available > 0 && name_chars.len() > available {
            let keep_start = available / 2;
            let keep_end = available - keep_start;

            let start_part: String = name_chars[..keep_start].iter().collect();
            let end_part: String = name_chars[name_chars.len() - keep_end..].iter().collect();

            format!("{}...{}{}", start_part, end_part, ext_part)
        } else {
            text.to_string()
        }
    } else {
        let chars: Vec<char> = text.chars().collect();
        let available = max_len.saturating_sub(3);
        let keep_start = available / 2;
        let keep_end = available - keep_start;

        let start_part: String = chars[..keep_start].iter().collect();
        let end_part: String = chars[chars.len() - keep_end..].iter().collect();

        format!("{}...{}", start_part, end_part)
    }
}

/// クエリにマッチするバイト範囲を返す（大文字小文字無視、連続部分文字列マッチ）
pub fn find_query_match_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    if query.is_empty() {
        return ranges;
    }

    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let text_chars: Vec<(usize, char)> = text.char_indices().collect();

    let mut i = 0;
    while i < text_chars.len() {
        let mut match_found = true;
        let mut q_idx = 0;
        let mut current_t_offset = 0;

        while q_idx < query_lower.len() {
            if i + current_t_offset >= text_chars.len() {
                match_found = false;
                break;
            }

            let (_, t_char) = text_chars[i + current_t_offset];
            let t_lower = t_char.to_lowercase();

            for tc in t_lower {
                if q_idx >= query_lower.len() || query_lower[q_idx] != tc {
                    match_found = false;
                    break;
                }
                q_idx += 1;
            }

            if !match_found {
                break;
            }
            current_t_offset += 1;
        }

        if match_found && q_idx == query_lower.len() {
            let start_byte = text_chars[i].0;
            let end_byte = if i + current_t_offset < text_chars.len() {
                text_chars[i + current_t_offset].0
            } else {
                text.len()
            };

            // char boundary 検証
            if text.is_char_boundary(start_byte) && text.is_char_boundary(end_byte) {
                ranges.push(start_byte..end_byte);
            }

            i += current_t_offset;
        } else {
            i += 1;
        }
    }

    ranges
}
