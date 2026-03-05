use nohrs::core::text::{find_query_match_ranges, truncate_middle};

// --- truncate_middle ---

#[test]
fn test_truncate_middle_short_text() {
    assert_eq!(truncate_middle("hello.txt", 20), "hello.txt");
}

#[test]
fn test_truncate_middle_exact_length() {
    let text = "hello.txt";
    assert_eq!(truncate_middle(text, text.chars().count()), "hello.txt");
}

#[test]
fn test_truncate_middle_with_extension() {
    let result = truncate_middle("very_long_filename_here.txt", 15);
    assert!(result.contains("..."));
    assert!(result.ends_with(".txt"));
    assert!(result.chars().count() <= 15);
}

#[test]
fn test_truncate_middle_without_extension() {
    let result = truncate_middle("VeryLongFileNameWithoutExtension", 15);
    assert!(result.contains("..."));
    assert!(result.chars().count() <= 15);
}

#[test]
fn test_truncate_middle_multibyte() {
    let result = truncate_middle("日本語のとても長いファイル名.txt", 15);
    assert!(result.contains("..."));
    assert!(result.ends_with(".txt"));
    assert!(result.chars().count() <= 15);
}

#[test]
fn test_truncate_middle_very_short_max() {
    let result = truncate_middle("hello.txt", 3);
    // max_len < 4 の場合は先頭 max_len 文字を返す
    assert_eq!(result.chars().count(), 3);
}

// --- find_query_match_ranges ---

#[test]
fn test_find_query_match_ranges_basic() {
    let ranges = find_query_match_ranges("hello world", "hello");
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], 0..5);
}

#[test]
fn test_find_query_match_ranges_case_insensitive() {
    let ranges = find_query_match_ranges("Hello World", "hello");
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], 0..5);
}

#[test]
fn test_find_query_match_ranges_multiple_matches() {
    let ranges = find_query_match_ranges("foo bar foo baz foo", "foo");
    assert_eq!(ranges.len(), 3);
    assert_eq!(ranges[0], 0..3);
    assert_eq!(ranges[1], 8..11);
    assert_eq!(ranges[2], 16..19);
}

#[test]
fn test_find_query_match_ranges_empty_query() {
    let ranges = find_query_match_ranges("hello", "");
    assert!(ranges.is_empty());
}

#[test]
fn test_find_query_match_ranges_multibyte() {
    let text = "日本語テスト";
    let ranges = find_query_match_ranges(text, "テスト");
    assert_eq!(ranges.len(), 1);
    // "日本語" is 9 bytes (3 bytes each), "テスト" starts at byte 9
    assert_eq!(ranges[0].start, 9);
    assert_eq!(&text[ranges[0].clone()], "テスト");
}

#[test]
fn test_find_query_match_ranges_no_match() {
    let ranges = find_query_match_ranges("hello world", "xyz");
    assert!(ranges.is_empty());
}

#[test]
fn test_find_query_match_ranges_partial_overlap() {
    let ranges = find_query_match_ranges("aaa", "aa");
    // "aa" matches at position 0 (bytes 0..2), then next search starts at 2
    // so only one match at position 0..2
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], 0..2);
}

#[test]
fn test_find_query_match_ranges_whole_string() {
    let ranges = find_query_match_ranges("abc", "abc");
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], 0..3);
}

#[test]
fn test_find_query_match_ranges_query_longer_than_text() {
    let ranges = find_query_match_ranges("ab", "abcdef");
    assert!(ranges.is_empty());
}
