/// Tier 0: filesystem path/file completions.
///
/// Triggered when the last token of `editor_text` starts with `/`, `./`,
/// `../`, or `~`, or when the command is `cd` (completes directories from the
/// current working directory).  Returns fully-reconstructed command strings
/// sorted alphabetically.  Hidden entries are excluded unless the user typed a
/// leading `.`.
pub(super) fn path_completions(editor_text: &str, cwd: &str) -> Vec<String> {
    if editor_text.is_empty() {
        return Vec::new();
    }
    let is_cd = editor_text == "cd" || editor_text.starts_with("cd ");

    // Split into the fixed command part and the path fragment being completed.
    let (cmd_part, raw_frag) = match editor_text.rfind(' ') {
        Some(pos) => (&editor_text[..=pos], &editor_text[pos + 1..]),
        None => ("", editor_text),
    };

    let is_path_like = raw_frag.starts_with('/')
        || raw_frag.starts_with("./")
        || raw_frag.starts_with("../")
        || raw_frag.starts_with('~');

    if !is_path_like && !is_cd {
        return Vec::new();
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let expanded: String = if let Some(rest) = raw_frag.strip_prefix('~') {
        format!("{}{}", home, rest)
    } else if let Some(rest) = raw_frag.strip_prefix("./") {
        format!("{}/{}", cwd.trim_end_matches('/'), rest)
    } else if let Some(rest) = raw_frag.strip_prefix("../") {
        let parent = std::path::Path::new(cwd)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_string());
        format!("{}/{}", parent.trim_end_matches('/'), rest)
    } else if raw_frag.starts_with('/') {
        raw_frag.to_string()
    } else {
        // Relative path (cd without explicit ./ prefix): resolve against cwd.
        format!("{}/{}", cwd.trim_end_matches('/'), raw_frag)
    };

    let (dir_to_read, name_prefix) = match expanded.rfind('/') {
        Some(pos) => (&expanded[..=pos], &expanded[pos + 1..]),
        None => (cwd, expanded.as_str()),
    };

    let entries = match std::fs::read_dir(dir_to_read) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    // Keep the user-typed directory sigil (e.g. "~/", "./") in the output.
    let display_dir = match raw_frag.rfind('/') {
        Some(pos) => &raw_frag[..=pos],
        None => "",
    };

    let name_lower = name_prefix.to_lowercase();
    let dirs_only = is_cd;

    let mut completions: Vec<String> = entries
        .filter_map(|res| res.ok())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Skip hidden entries unless the user explicitly typed a leading '.'.
            if name.starts_with('.') && !name_prefix.starts_with('.') {
                return false;
            }
            name.to_lowercase().starts_with(&name_lower)
        })
        .filter(|entry| !dirs_only || entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let trailer = if is_dir { "/" } else { "" };
            format!("{}{}{}{}", cmd_part, display_dir, name, trailer)
        })
        .collect();

    completions.sort();
    completions
}
