use super::types::SortKey;
use crate::services::fs::listing::FileEntryDto;

pub fn sort_entries(entries: &mut [FileEntryDto], key: SortKey, asc: bool) {
    entries.sort_by(|a, b| {
        // Directories before files
        let is_dir_a = a.kind == "dir";
        let is_dir_b = b.kind == "dir";

        match is_dir_b.cmp(&is_dir_a) {
            std::cmp::Ordering::Equal => {
                let order = match key {
                    SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortKey::Size => a.size.cmp(&b.size),
                    SortKey::Modified => a.modified.cmp(&b.modified),
                    SortKey::Type => {
                        let ext_a = get_extension(&a.name, &a.kind);
                        let ext_b = get_extension(&b.name, &b.kind);
                        ext_a.cmp(&ext_b)
                    }
                };
                if asc {
                    order
                } else {
                    order.reverse()
                }
            }
            kind_order => kind_order,
        }
    });
}

pub fn get_extension(name: &str, kind: &str) -> String {
    match kind {
        "dir" => "0_dir".to_string(),
        "file" => std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| "zzz_noext".to_string()),
        other => other.to_string(),
    }
}
