//! Shell-native dynamic completions (git branches, SSH hosts, Docker containers, …).
//!
//! Uses the same async-background-fetch + TTL-cache model as `manpage.rs`.
//! Stale entries are returned immediately while a background refresh runs, so
//! the UI never blocks.  The public entry point is [`dynamic_completions`].

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(10);

// ─── Cache ────────────────────────────────────────────────────────────────────

struct Entry {
    items: Vec<String>,
    fetched_at: Instant,
}

fn cache() -> &'static Mutex<HashMap<String, Entry>> {
    static C: std::sync::OnceLock<Mutex<HashMap<String, Entry>>> = std::sync::OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn loading() -> &'static Mutex<HashSet<String>> {
    static L: std::sync::OnceLock<Mutex<HashSet<String>>> = std::sync::OnceLock::new();
    L.get_or_init(|| Mutex::new(HashSet::new()))
}

fn get_cached(key: &str) -> Option<Vec<String>> {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(key)
        .map(|e| e.items.clone())
}

fn is_stale(key: &str) -> bool {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(key)
        .map(|e| e.fetched_at.elapsed() > CACHE_TTL)
        .unwrap_or(true)
}

fn store_cached(key: String, items: Vec<String>) {
    cache().lock().unwrap_or_else(|e| e.into_inner()).insert(
        key,
        Entry {
            items,
            fetched_at: Instant::now(),
        },
    );
}

/// Returns `false` if a fetch is already in-flight for `key`.
fn mark_loading(key: &str) -> bool {
    loading()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.to_string())
}

fn unmark_loading(key: &str) {
    loading()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(key);
}

fn spawn_fetch(key: String, fetch: impl FnOnce() -> Vec<String> + Send + 'static) {
    if !mark_loading(&key) {
        return;
    }
    std::thread::spawn(move || {
        let items = fetch();
        store_cached(key.clone(), items);
        unmark_loading(&key);
    });
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Returns live environment completions for `prefix` typed in directory `cwd`
/// under `shell`.  Results arrive asynchronously — empty on the very first call,
/// then populated after a background fetch.  Stale cached entries are returned
/// while a refresh runs.
pub(super) fn dynamic_completions(prefix: &str, cwd: &str, shell: &str) -> Vec<String> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let tokens: Vec<&str> = prefix.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    match tokens[0] {
        "git" if tokens.len() >= 2 || prefix.ends_with(' ') => {
            results.extend(git_completions(prefix, cwd, &tokens));
        }
        "ssh" => results.extend(ssh_completions(prefix, &tokens)),
        "docker" | "podman" => {
            results.extend(docker_completions(prefix, tokens[0], &tokens));
        }
        _ => {}
    }

    // Fish-native completions cover everything else (kubectl, npm, cargo, etc.)
    // Only used for multi-token prefixes where native completions add real value.
    if shell.contains("fish") && prefix.contains(' ') {
        results.extend(fish_completions(prefix, cwd));
    }

    // Deduplicate while preserving order (hardcoded providers may overlap with fish).
    let mut seen = HashSet::new();
    results.retain(|s| seen.insert(s.to_lowercase()));

    results
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Runs a command and collects non-empty trimmed stdout lines.
fn run_lines(args: &[&str]) -> Vec<String> {
    let Some((cmd, rest)) = args.split_first() else {
        return Vec::new();
    };
    std::process::Command::new(cmd)
        .args(rest)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Splits `prefix` into the stable part (everything up to and including the
/// last space) and the incomplete last token.
///
/// `"git checkout "` → `("git checkout ", "")`
/// `"git checkout mai"` → `("git checkout ", "mai")`
/// `"git"` → `("", "git")`
fn split_prefix(prefix: &str) -> (&str, &str) {
    if prefix.ends_with(' ') {
        (prefix, "")
    } else {
        match prefix.rfind(' ') {
            Some(i) => (&prefix[..=i], &prefix[i + 1..]),
            None => ("", prefix),
        }
    }
}

/// Builds full command strings from bare completion `tokens`, filtered by
/// `last_token` prefix, and prefixed with `stable`.
fn build_completions(stable: &str, last_token: &str, tokens: &[String]) -> Vec<String> {
    let lower = last_token.to_lowercase();
    tokens
        .iter()
        .filter(|t| {
            let tl = t.to_lowercase();
            tl.starts_with(&lower) && t.as_str() != last_token
        })
        .map(|t| format!("{stable}{t}"))
        .collect()
}

// ─── Git ─────────────────────────────────────────────────────────────────────

fn git_completions(prefix: &str, cwd: &str, tokens: &[&str]) -> Vec<String> {
    // Need at least "git <subcommand>" before we know what to complete.
    let sub = match tokens.get(1) {
        Some(s) => *s,
        None => return Vec::new(),
    };
    let (stable, last_token) = split_prefix(prefix);
    if last_token.starts_with('-') {
        return Vec::new();
    }

    match sub {
        "checkout" | "switch" | "merge" | "rebase" | "cherry-pick" | "diff" | "show" | "log"
        | "reset" | "restore" => {
            let mut items = git_branches(cwd);
            items.extend(git_tags(cwd));
            items.dedup();
            build_completions(stable, last_token, &items)
        }
        "push" | "pull" | "fetch" => build_completions(stable, last_token, &git_remotes(cwd)),
        "branch" => build_completions(stable, last_token, &git_branches(cwd)),
        "tag" if tokens.len() <= 3 => build_completions(stable, last_token, &git_tags(cwd)),
        _ => Vec::new(),
    }
}

fn git_branches(cwd: &str) -> Vec<String> {
    let key = format!("git:branches:{cwd}");
    let cached = get_cached(&key);
    if is_stale(&key) {
        let cwd = cwd.to_string();
        spawn_fetch(key, move || {
            run_lines(&[
                "git",
                "-C",
                &cwd,
                "branch",
                "-a",
                "--format=%(refname:short)",
            ])
        });
    }
    cached.unwrap_or_default()
}

fn git_tags(cwd: &str) -> Vec<String> {
    let key = format!("git:tags:{cwd}");
    let cached = get_cached(&key);
    if is_stale(&key) {
        let cwd = cwd.to_string();
        spawn_fetch(key, move || run_lines(&["git", "-C", &cwd, "tag"]));
    }
    cached.unwrap_or_default()
}

fn git_remotes(cwd: &str) -> Vec<String> {
    let key = format!("git:remotes:{cwd}");
    let cached = get_cached(&key);
    if is_stale(&key) {
        let cwd = cwd.to_string();
        spawn_fetch(key, move || run_lines(&["git", "-C", &cwd, "remote"]));
    }
    cached.unwrap_or_default()
}

// ─── SSH ─────────────────────────────────────────────────────────────────────

fn ssh_completions(prefix: &str, tokens: &[&str]) -> Vec<String> {
    let (stable, last_token) = split_prefix(prefix);
    // Don't complete when the last token is a flag or there are already flags/options.
    if last_token.starts_with('-') || tokens.iter().any(|t| t.starts_with('-')) {
        return Vec::new();
    }
    // Only complete the first non-command token (the hostname).
    if tokens.len() > 2 || (tokens.len() == 2 && prefix.ends_with(' ')) {
        return Vec::new();
    }
    build_completions(stable, last_token, &ssh_hosts())
}

fn ssh_hosts() -> Vec<String> {
    let key = "ssh:hosts".to_string();
    let cached = get_cached(&key);
    if is_stale(&key) {
        spawn_fetch(key, || {
            let mut hosts: HashSet<String> = HashSet::new();
            let home = std::env::var("HOME").unwrap_or_default();

            // ~/.ssh/known_hosts — skip hashed (|1|…) entries.
            let kh_path = format!("{home}/.ssh/known_hosts");
            if let Ok(text) = std::fs::read_to_string(&kh_path) {
                for line in text.lines() {
                    if line.starts_with('#') || line.starts_with('|') || line.is_empty() {
                        continue;
                    }
                    if let Some(host_field) = line.split_whitespace().next() {
                        for h in host_field.split(',') {
                            // Strip bracketed [host]:port notation.
                            let h = h.trim_start_matches('[').split(']').next().unwrap_or(h);
                            if !h.is_empty() && !h.contains('*') {
                                hosts.insert(h.to_string());
                            }
                        }
                    }
                }
            }

            // ~/.ssh/config Host directives.
            let cfg_path = format!("{home}/.ssh/config");
            if let Ok(text) = std::fs::read_to_string(&cfg_path) {
                for line in text.lines() {
                    let t = line.trim();
                    if let Some(rest) = t.strip_prefix("Host ").or_else(|| t.strip_prefix("host "))
                    {
                        for h in rest.split_whitespace() {
                            if !h.contains('*') && !h.contains('?') {
                                hosts.insert(h.to_string());
                            }
                        }
                    }
                }
            }

            let mut v: Vec<String> = hosts.into_iter().collect();
            v.sort();
            v
        });
    }
    cached.unwrap_or_default()
}

// ─── Docker / Podman ─────────────────────────────────────────────────────────

fn docker_completions(prefix: &str, cmd: &str, tokens: &[&str]) -> Vec<String> {
    let sub = match tokens.get(1) {
        Some(s) => *s,
        None => return Vec::new(),
    };
    let (stable, last_token) = split_prefix(prefix);
    if last_token.starts_with('-') {
        return Vec::new();
    }

    match sub {
        "stop" | "start" | "restart" | "rm" | "exec" | "inspect" | "logs" | "kill" | "pause"
        | "unpause" | "attach" | "top" | "stats" => {
            build_completions(stable, last_token, &docker_containers(cmd))
        }
        "rmi" | "tag" | "save" | "history" | "run" => {
            build_completions(stable, last_token, &docker_images(cmd))
        }
        _ => Vec::new(),
    }
}

fn docker_containers(cmd: &str) -> Vec<String> {
    let key = format!("{cmd}:containers");
    let cached = get_cached(&key);
    if is_stale(&key) {
        let cmd = cmd.to_string();
        spawn_fetch(key, move || {
            run_lines(&[&cmd, "ps", "--format", "{{.Names}}"])
        });
    }
    cached.unwrap_or_default()
}

fn docker_images(cmd: &str) -> Vec<String> {
    let key = format!("{cmd}:images");
    let cached = get_cached(&key);
    if is_stale(&key) {
        let cmd = cmd.to_string();
        spawn_fetch(key, move || {
            run_lines(&[&cmd, "images", "--format", "{{.Repository}}:{{.Tag}}"])
                .into_iter()
                .filter(|s| !s.contains("<none>"))
                .collect()
        });
    }
    cached.unwrap_or_default()
}

// ─── Fish ─────────────────────────────────────────────────────────────────────

fn fish_completions(prefix: &str, cwd: &str) -> Vec<String> {
    let (stable, last_token) = split_prefix(prefix);
    // Cache by the stable prefix so typing one more character hits the cache
    // instead of spawning a new fish process.
    let key = format!("fish:{stable}:{cwd}");
    let cached = get_cached(&key);
    if is_stale(&key) {
        let stable_owned = stable.to_string();
        let cwd_owned = cwd.to_string();
        spawn_fetch(key, move || fetch_fish_tokens(&stable_owned, &cwd_owned));
    }
    let lower_last = last_token.to_lowercase();
    cached
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t.to_lowercase().starts_with(&lower_last) && t.as_str() != last_token)
        .map(|t| format!("{stable}{t}"))
        .collect()
}

/// Wraps `s` in double quotes, escaping `\`, `"`, and `$` for fish.
fn fish_quote(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$");
    format!("\"{escaped}\"")
}

/// Runs `fish -c "complete --do-complete STABLE_PREFIX"` and returns bare
/// completion tokens (the part after the stable prefix, tab-description stripped).
fn fetch_fish_tokens(stable_prefix: &str, cwd: &str) -> Vec<String> {
    let script = format!("complete --do-complete {}", fish_quote(stable_prefix));
    let Ok(out) = std::process::Command::new("fish")
        .args(["--command", &script])
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            // Fish outputs "completion\tdescription" — keep only the completion token.
            line.split('\t').next().unwrap_or(line).to_string()
        })
        .collect()
}
