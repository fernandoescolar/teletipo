/// Lazily-collected, deduplicated list of available command names from man1
/// directories and every directory in $PATH.  Populated asynchronously on first
/// call; returns an empty slice until the background scan completes.
pub(super) fn man_commands() -> &'static [String] {
    static CMDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    if let Some(cmds) = CMDS.get() {
        return cmds;
    }
    static SCANNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !SCANNING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        std::thread::spawn(|| {
            let _ = CMDS.set(collect_all_commands());
        });
    }
    &[]
}

/// Collects command names from man1 directories and every $PATH directory.
fn collect_all_commands() -> Vec<String> {
    let mut cmds: std::collections::HashSet<String> = std::collections::HashSet::new();

    // man1 directories.
    for dir in &[
        "/usr/share/man/man1",
        "/usr/local/share/man/man1",
        "/opt/homebrew/share/man/man1",
    ] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let fname = entry.file_name().to_string_lossy().into_owned();
            let without_gz = fname.trim_end_matches(".gz");
            let cmd = if let Some(dot) = without_gz.rfind('.') {
                &without_gz[..dot]
            } else {
                without_gz
            };
            if !cmd.is_empty() && !cmd.contains('/') {
                cmds.insert(cmd.to_string());
            }
        }
    }

    // $PATH executables — includes scripts and tools without man pages.
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(meta) = entry.metadata() {
                    if is_executable_path(&entry.path(), &meta) {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if !name.is_empty() && !name.contains('/') && !name.starts_with('.') {
                            cmds.insert(name);
                        }
                    }
                }
            }
        }
    }

    let mut v: Vec<String> = cmds.into_iter().collect();
    v.sort();
    v
}

#[cfg(unix)]
fn is_executable_path(path: &std::path::Path, meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.is_file() && (meta.permissions().mode() & 0o111 != 0) && path.file_name().is_some()
}

#[cfg(not(unix))]
fn is_executable_path(path: &std::path::Path, meta: &std::fs::Metadata) -> bool {
    if !meta.is_file() {
        return false;
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let ext_upper = ext.to_ascii_uppercase();
            ["EXE", "BAT", "CMD", "COM", "PS1"].contains(&ext_upper.as_str())
        }
        None => false,
    }
}

// ── Unified asynchronous man-page data cache ─────────────────────────────────
pub(super) struct ManData {
    pub flags: Vec<String>,
    pub subcommands: Vec<String>,
}

type ManDataMap = std::collections::HashMap<String, ManData>;

fn man_data_cache() -> &'static std::sync::Mutex<ManDataMap> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<ManDataMap>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Trigger asynchronous loading of man-page data for `cmd` if not yet cached.
fn ensure_man_loaded(cmd: &str) {
    if man_data_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(cmd)
    {
        return;
    }
    static LOADING: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    let loading = LOADING.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    {
        let mut guard = loading.lock().unwrap_or_else(|e| e.into_inner());
        if !guard.insert(cmd.to_string()) {
            return;
        }
    }
    let cmd_owned = cmd.to_string();
    std::thread::spawn(move || {
        let data = fetch_man_data(&cmd_owned);
        man_data_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(cmd_owned.clone(), data);
        LOADING
            .get()
            .expect("initialized by get_or_init above")
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&cmd_owned);
    });
}

fn fetch_man_data(cmd: &str) -> ManData {
    let output = std::process::Command::new("man")
        .args(["-P", "cat", cmd])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();
    let (flags, mut subcommands) = match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            (
                extract_flags_from_text(&text),
                extract_subcommands_from_text(cmd, &text),
            )
        }
        Err(_) => (Vec::new(), Vec::new()),
    };
    if subcommands.is_empty() {
        subcommands = fetch_help_subcommands(cmd);
    }
    ManData { flags, subcommands }
}

fn extract_flags_from_text(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut flags: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
        if trimmed.len() < 2 || !trimmed.starts_with('-') {
            continue;
        }
        let rest = trimmed.trim_start_matches('-');
        if rest.is_empty() {
            continue;
        }
        if rest
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && seen.insert(trimmed.to_string())
        {
            flags.push(trimmed.to_string());
        }
    }
    flags.sort();
    flags
}

fn extract_subcommands_from_text(cmd: &str, text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut subs: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let rest = match trimmed
            .strip_prefix(cmd)
            .filter(|r| r.starts_with(' ') || r.starts_with('\t'))
        {
            Some(r) => r.trim_start(),
            None => continue,
        };
        let word = rest.split_whitespace().next().unwrap_or("");
        if word.len() < 2 || word.starts_with('-') || word.starts_with('[') || word.starts_with('<')
        {
            continue;
        }
        if word.chars().all(|c| c.is_alphanumeric() || c == '-') && seen.insert(word.to_string()) {
            subs.push(word.to_string());
        }
    }
    subs.sort();
    subs
}

/// Run `<cmd> --help` and parse structured subcommand sections.
fn fetch_help_subcommands(cmd: &str) -> Vec<String> {
    let out = match std::process::Command::new(cmd)
        .arg("--help")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let text: &str = if stdout.len() >= stderr.len() {
        &stdout
    } else {
        &stderr
    };
    parse_help_text_subcommands(text)
}

/// Parse subcommand listings from `--help` output.
/// Recognises section headers containing "command"/"subcommand", then collects
/// indented lines of the form `  <word>   <description>`.
fn parse_help_text_subcommands(text: &str) -> Vec<String> {
    let mut in_section = false;
    let mut seen = std::collections::HashSet::new();
    let mut subs: Vec<String> = Vec::new();
    for line in text.lines() {
        let lower = line.to_lowercase();
        let trimmed_lower = lower.trim();
        let is_header = !line.starts_with(' ')
            && !line.starts_with('\t')
            && !line.trim().is_empty()
            && (trimmed_lower.contains("subcommand")
                || trimmed_lower == "commands"
                || trimmed_lower == "commands:"
                || trimmed_lower.starts_with("available command"));
        if is_header {
            in_section = true;
            continue;
        }
        if !line.is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            in_section = false;
            continue;
        }
        if !in_section || line.trim().is_empty() {
            continue;
        }
        let stripped = line.trim_start();
        let indent = line.len() - stripped.len();
        if indent < 2 {
            continue;
        }
        let word = stripped.split_whitespace().next().unwrap_or("");
        if word.len() < 2 || word.starts_with('-') || word.starts_with('[') || word.starts_with('<')
        {
            continue;
        }
        if word
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && seen.insert(word.to_string())
        {
            subs.push(word.to_string());
        }
    }
    subs.sort();
    subs
}

pub(super) fn man_flags(cmd: &str) -> Vec<String> {
    ensure_man_loaded(cmd);
    man_data_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(cmd)
        .map(|d| d.flags.clone())
        .unwrap_or_default()
}

pub(super) fn man_subcommands(cmd: &str) -> Vec<String> {
    ensure_man_loaded(cmd);
    man_data_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(cmd)
        .map(|d| d.subcommands.clone())
        .unwrap_or_default()
}

/// Return subcommands for a multi-word command (e.g. "git remote").
/// Tries `man <base>-<sub>` first, then `<cmd> <sub> --help`.
/// Results are cached asynchronously — returns empty on the first call.
pub(super) fn nested_subcommands(multi_cmd: &str) -> Vec<String> {
    type Cache = std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>;
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    static LOADING: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    fn get_cache() -> &'static Cache {
        CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
    }
    {
        let guard = get_cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(subs) = guard.get(multi_cmd) {
            return subs.clone();
        }
    }
    let loading = LOADING.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    {
        let mut guard = loading.lock().unwrap_or_else(|e| e.into_inner());
        if !guard.insert(multi_cmd.to_string()) {
            return Vec::new();
        }
    }
    let key = multi_cmd.to_string();
    std::thread::spawn(move || {
        let tokens: Vec<&str> = key.split_whitespace().collect();
        let subs = if tokens.len() >= 2 {
            let hyphenated = tokens.join("-");
            let man_text = std::process::Command::new("man")
                .args(["-P", "cat", &hyphenated])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            let from_man = extract_subcommands_from_text(&hyphenated, &man_text);
            if !from_man.is_empty() {
                from_man
            } else {
                match std::process::Command::new(tokens[0])
                    .args(&tokens[1..])
                    .arg("--help")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .output()
                {
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
                        let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                        let text = if stdout.len() >= stderr.len() {
                            stdout
                        } else {
                            stderr
                        };
                        parse_help_text_subcommands(&text)
                    }
                    Err(_) => Vec::new(),
                }
            }
        } else {
            Vec::new()
        };
        get_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.clone(), subs);
        LOADING
            .get()
            .expect("initialized by get_or_init above")
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    });
    Vec::new()
}
