use anyhow::Result;
use nohrs::services::search::SearchBackend;
use nohrs::services::search::ripgrep::RipgrepBackend;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_ripgrep_basic_search() -> Result<()> {
    let tmp = tempdir()?;
    fs::write(tmp.path().join("test.txt"), "Hello world")?;

    let backend = RipgrepBackend::new(tmp.path().to_path_buf());
    let results = backend.search("Hello")?;

    assert!(!results.is_empty(), "Should find 'Hello'");
    assert!(results[0].line_content.contains("Hello"));
    Ok(())
}

#[test]
fn test_ripgrep_case_sensitive() -> Result<()> {
    let tmp = tempdir()?;
    fs::write(tmp.path().join("test.txt"), "Hello world")?;

    let backend = RipgrepBackend::new(tmp.path().to_path_buf());

    // ripgrep regex is case-sensitive by default
    let results_upper = backend.search("Hello")?;
    assert!(!results_upper.is_empty(), "Should find 'Hello' (case match)");

    // lowercase should not match since regex is case-sensitive
    let results_lower = backend.search("hello")?;
    assert!(
        results_lower.is_empty(),
        "Should NOT find 'hello' (case-sensitive)"
    );
    Ok(())
}

#[test]
fn test_ripgrep_binary_skipped() -> Result<()> {
    let tmp = tempdir()?;
    // Binary content with null bytes
    fs::write(tmp.path().join("binary.bin"), b"\x00\x01\x02Hello\x00")?;
    // Text file for comparison
    fs::write(tmp.path().join("text.txt"), "Hello world")?;

    let backend = RipgrepBackend::new(tmp.path().to_path_buf());
    let results = backend.search("Hello")?;

    // Only the text file should match
    let text_matches: Vec<_> = results
        .iter()
        .filter(|r| r.path.to_string_lossy().contains("text.txt"))
        .collect();
    assert!(
        !text_matches.is_empty(),
        "Should find 'Hello' in text file"
    );

    let binary_matches: Vec<_> = results
        .iter()
        .filter(|r| r.path.to_string_lossy().contains("binary.bin"))
        .collect();
    assert!(
        binary_matches.is_empty(),
        "Should skip binary files"
    );
    Ok(())
}

#[test]
fn test_ripgrep_line_number_and_content() -> Result<()> {
    let tmp = tempdir()?;
    fs::write(
        tmp.path().join("multi.txt"),
        "line one\nline two target\nline three\n",
    )?;

    let backend = RipgrepBackend::new(tmp.path().to_path_buf());
    let results = backend.search("target")?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].line_number, 2);
    assert!(results[0].line_content.contains("target"));
    Ok(())
}

#[test]
fn test_ripgrep_gitignore_respected() -> Result<()> {
    let tmp = tempdir()?;

    // Initialize git repo structure
    fs::create_dir_all(tmp.path().join(".git"))?;
    fs::write(tmp.path().join(".gitignore"), "ignored/\n")?;
    fs::create_dir_all(tmp.path().join("ignored"))?;
    fs::write(tmp.path().join("ignored/file.txt"), "secret content")?;
    fs::write(tmp.path().join("visible.txt"), "visible content")?;

    let backend = RipgrepBackend::new(tmp.path().to_path_buf());
    let results = backend.search("content")?;

    let visible: Vec<_> = results
        .iter()
        .filter(|r| r.path.to_string_lossy().contains("visible"))
        .collect();
    assert!(!visible.is_empty(), "Should find content in visible file");

    let ignored: Vec<_> = results
        .iter()
        .filter(|r| r.path.to_string_lossy().contains("ignored"))
        .collect();
    assert!(
        ignored.is_empty(),
        "Should NOT find content in gitignored file"
    );
    Ok(())
}
