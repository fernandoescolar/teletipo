use std::fs;
use std::path::{Path, PathBuf};

/// Extract potential path from text (last word if it looks like a path)
pub fn extract_path_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    // Split by spaces and get the last part
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let last_word = parts[parts.len() - 1];

    // Check if it looks like a path
    if last_word.contains('/') || last_word.starts_with('~') || last_word.starts_with('.') {
        Some(last_word.to_string())
    } else {
        None
    }
}

/// Get completions for a partial path
pub fn get_path_completions(partial: &str, tab_cwd: &str) -> Vec<String> {
    let path = expand_path(partial, tab_cwd);

    // Determine if we're completing a directory or looking inside it
    let (dir_to_search, prefix) = if partial.ends_with('/') {
        // User typed "folder/" — list contents of "folder"
        (path.clone(), String::new())
    } else if let Some(parent) = Path::new(&path).parent() {
        // User typed "folder/par" — list contents of "folder" starting with "par"
        let parent_str = parent.to_string_lossy().to_string();
        if parent_str == "." && !partial.starts_with("./") && !partial.starts_with("../") {
            (tab_cwd.to_string(), partial.to_string())
        } else {
            (parent_str, get_filename(&path))
        }
    } else {
        // No parent — search in tab cwd with prefix
        (tab_cwd.to_string(), partial.to_string())
    };

    // Try to read directory
    if let Ok(entries) = fs::read_dir(&dir_to_search) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Filter by prefix (case-insensitive)
            if !prefix.is_empty() && !name_str.to_lowercase().starts_with(&prefix.to_lowercase()) {
                continue;
            }

            // Skip hidden files unless explicitly searching for them
            if name_str.starts_with('.') && !prefix.starts_with('.') {
                continue;
            }

            let is_dir = entry.metadata().ok().map(|m| m.is_dir()).unwrap_or(false);

            if is_dir {
                // Add directories with trailing /
                dirs.push(format!("{}/", name_str));
            } else {
                // Add files
                files.push(name_str.to_string());
            }
        }

        // Sort each group and combine: directories first, then files
        dirs.sort();
        files.sort();
        dirs.extend(files);
        dirs
    } else {
        Vec::new()
    }
}

/// Expand ~ and make paths absolute relative to tab_cwd
fn expand_path(path: &str, tab_cwd: &str) -> String {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            home.join(stripped).to_string_lossy().to_string()
        } else {
            path.to_string()
        }
    } else if let Some(stripped) = path.strip_prefix("./") {
        PathBuf::from(tab_cwd)
            .join(stripped)
            .to_string_lossy()
            .to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        PathBuf::from(tab_cwd)
            .join(path)
            .to_string_lossy()
            .to_string()
    }
}

/// Get filename from path (last component)
fn get_filename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}
