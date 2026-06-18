mod dynamic;
mod manpage;
mod paths;
mod tiered;

use manpage::{man_commands, man_flags, man_subcommands, nested_subcommands};
use paths::path_completions;

pub(crate) use tiered::suggestion_matches_tiered;

use crate::tab;

fn tier0b_completions(prefix: &str) -> Vec<String> {
    if !prefix.contains(' ') {
        return Vec::new();
    }
    let last_sp = prefix
        .rfind(' ')
        .expect("contains ' ' — guarded by check above");
    let last_token = &prefix[last_sp + 1..];
    let base_cmd = prefix.split_whitespace().next().unwrap_or("");
    let cmd_fixed = &prefix[..=last_sp];
    if last_token.starts_with('-') {
        man_flags(base_cmd)
            .iter()
            .filter(|flag| flag.starts_with(last_token) && flag.as_str() != last_token)
            .map(|flag| format!("{}{}", cmd_fixed, flag))
            .collect()
    } else {
        let lower_token = last_token.to_lowercase();
        let cmd_tokens: Vec<&str> = cmd_fixed.split_whitespace().collect();
        let source_subs: Vec<String> = if cmd_tokens.len() >= 2 {
            let nested = nested_subcommands(&cmd_tokens.join(" "));
            if !nested.is_empty() {
                nested
            } else {
                man_subcommands(base_cmd)
            }
        } else {
            man_subcommands(base_cmd)
        };
        source_subs
            .iter()
            .filter(|sub| {
                let sl = sub.to_lowercase();
                (lower_token.is_empty() || sl.starts_with(&lower_token))
                    && sub.as_str() != last_token
            })
            .take(20)
            .map(|sub| format!("{}{}", cmd_fixed, sub))
            .collect()
    }
}

fn cd_tier1_matches(history: &[String], prefix: &str) -> Vec<String> {
    let (raw_t1, _, _) = suggestion_matches_tiered(history, prefix);
    let mut cd_t1: Vec<String> = raw_t1
        .into_iter()
        .filter(|e| e.starts_with("cd "))
        .collect();
    let path_frag = prefix.strip_prefix("cd ").unwrap_or("");
    let bare_relative = !path_frag.is_empty()
        && !path_frag.starts_with('/')
        && !path_frag.starts_with('~')
        && !path_frag.starts_with("./")
        && !path_frag.starts_with("../");
    if bare_relative {
        let dot_prefix = format!("cd ./{}", path_frag);
        let (dot_t1, _, _) = suggestion_matches_tiered(history, &dot_prefix);
        let already: std::collections::HashSet<String> =
            cd_t1.iter().map(|s| s.to_lowercase()).collect();
        for entry in dot_t1 {
            if let Some(rest) = entry.strip_prefix("cd ./") {
                let normalized = format!("cd {}", rest);
                if !already.contains(&normalized.to_lowercase()) {
                    cd_t1.push(normalized);
                }
            }
        }
    }
    cd_t1
}

fn build_pool_ordered(
    t1: Vec<String>,
    t2: Vec<String>,
    tdyn: Vec<String>,
    t0b: Vec<String>,
    t4: Vec<String>,
) -> Vec<String> {
    let mut pool: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in t1.into_iter().chain(t2).chain(tdyn).chain(t0b).chain(t4) {
        if seen.insert(item.to_lowercase()) {
            pool.push(item);
        }
    }
    pool
}

fn apply_frecency_sort(pool: &mut [String], entries: &[tab::HistoryEntry]) {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let score_map: std::collections::HashMap<&str, f32> = entries
        .iter()
        .map(|e| {
            let elapsed_hours = (now_secs.saturating_sub(e.last_used_secs)) as f32 / 3_600.0;
            (e.cmd.as_str(), e.count as f32 / (1.0 + elapsed_hours))
        })
        .collect();
    let prefix_scores: std::collections::HashMap<String, f32> = {
        let mut ps: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
        for (&cmd_str, &score) in &score_map {
            let tokens: Vec<&str> = cmd_str.split_whitespace().collect();
            for n in 1..=tokens.len().min(3) {
                let key = tokens[..n].join(" ");
                let e = ps.entry(key).or_insert(0.0_f32);
                *e = e.max(score);
            }
        }
        ps
    };
    let score_for = |s: &str| -> f32 {
        if let Some(&sc) = score_map.get(s) {
            return sc;
        }
        prefix_scores.get(s).copied().unwrap_or(0.0)
    };
    pool.sort_by(|a, b| {
        let sa = score_for(a);
        let sb = score_for(b);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Like [`suggestion_matches_tiered`] but re-sorts results by frecency score
/// within each match-quality tier.  Also prepends filesystem path completions
/// (Tier 0) when `prefix` looks like a path or starts with `cd`, man-page flag
/// completions (Tier 0b) when the user is typing flags for a known command, and
/// appends man-page command names as a last resort (Tier 4) for single-word
/// prefixes.
pub(crate) fn suggestion_matches_frecency(
    history: &[String],
    entries: &[tab::HistoryEntry],
    prefix: &str,
    cwd: &str,
    shell: &str,
) -> Vec<String> {
    let is_cd = prefix == "cd" || prefix.starts_with("cd ");

    // Tier 0: filesystem path completions (alphabetical).
    let t0 = path_completions(prefix, cwd);

    // Tier 0b: man-page flag/subcommand completions.
    let t0b = if !is_cd {
        tier0b_completions(prefix)
    } else {
        Vec::new()
    };

    // Tiers 1-2: history-based matches.
    let (t1, t2) = if is_cd {
        (cd_tier1_matches(history, prefix), Vec::new())
    } else {
        let (a, b, _) = suggestion_matches_tiered(history, prefix);
        (a, b)
    };

    // Tier 3: shell-native dynamic completions (git branches, SSH hosts, …).
    let tdyn = if !is_cd {
        dynamic::dynamic_completions(prefix, cwd, shell)
    } else {
        Vec::new()
    };

    // Tier 4: man-page command names — single-word only, capped at 20.
    let t4: Vec<String> = if !is_cd && !prefix.contains(' ') && !prefix.is_empty() {
        let lower = prefix.to_lowercase();
        let already: std::collections::HashSet<String> = t0
            .iter()
            .chain(t0b.iter())
            .chain(t1.iter())
            .chain(t2.iter())
            .chain(tdyn.iter())
            .map(|s| s.to_lowercase())
            .collect();
        man_commands()
            .iter()
            .filter(|cmd| {
                let cl = cmd.to_lowercase();
                cl.starts_with(&lower) && cmd.as_str() != prefix && !already.contains(&cl)
            })
            .take(20)
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    let mut pool = build_pool_ordered(t1, t2, tdyn, t0b, t4);
    if entries.is_empty() {
        let mut out = t0;
        out.extend(pool);
        return out;
    }
    apply_frecency_sort(&mut pool, entries);
    let mut out = t0;
    out.extend(pool);
    out
}
