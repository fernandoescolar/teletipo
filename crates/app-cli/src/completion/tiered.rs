/// Return history matches split into three quality tiers (best first):
///
/// * **Tier 1** — full prefix match (case-insensitive): the entry *starts with*
///   the typed prefix.
/// * **Tier 2** — last-token match for multi-word prefixes: the command prefix
///   matches exactly and the typed last token is a prefix of the next word
///   (e.g. `"git ch"` → `"git cherry-pick"`).  Always starts with the typed prefix.
///
/// Each tier is deduplicated and ordered most-recently-used first.  An item
/// appearing in an earlier tier is excluded from later tiers.
pub(crate) fn suggestion_matches_tiered(
    history: &[String],
    prefix: &str,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    if prefix.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    // Normalize: trim every entry so commands stored with accidental leading/
    // trailing spaces still match a trimmed prefix.
    let normalized: Vec<String> = history.iter().map(|e| e.trim().to_string()).collect();
    let history: &[String] = &normalized;
    let lower = prefix.to_lowercase();

    // ── Tier 1: full prefix match ──────────────────────────────────────────
    // At most COMPLETION_MAX_PER_NEXT_TOKEN entries per "next-token group" to prevent
    // near-identical entries (e.g. 20 "git commit -m '…'" variants) from
    // flooding the dropdown and hiding other useful completions.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut next_tok_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let tier1: Vec<String> = history
        .iter()
        .rev()
        .filter(|e| {
            let el = e.to_lowercase();
            if !el.starts_with(&lower) || e.as_str() == prefix {
                return false;
            }
            if !seen.insert(el) {
                return false;
            }
            let rest = e.get(prefix.len()..).unwrap_or("").trim_start();
            let group = rest.split_whitespace().next().unwrap_or("").to_string();
            let n = next_tok_counts.entry(group).or_insert(0);
            if *n < crate::consts::COMPLETION_MAX_PER_NEXT_TOKEN {
                *n += 1;
                true
            } else {
                false
            }
        })
        .cloned()
        .collect();

    // ── Tier 2: last-token match (multi-word prefix) ──────────────────────
    // The command prefix must match exactly; the final typed token must be a
    // prefix of the corresponding token in the entry.  Every result starts
    // with the full typed prefix.
    let mut tier2: Vec<String> = Vec::new();
    if let Some(space_pos) = prefix.rfind(' ') {
        let fixed_lower = prefix[..=space_pos].to_lowercase();
        let last_token = prefix[space_pos + 1..].to_lowercase();
        if !last_token.is_empty() {
            tier2 = history
                .iter()
                .rev()
                .filter(|e| {
                    let el = e.to_lowercase();
                    if seen.contains(&el) || e.as_str() == prefix {
                        return false;
                    }
                    if !el.starts_with(&fixed_lower) {
                        return false;
                    }
                    let rest = &el[fixed_lower.len()..];
                    if rest.starts_with(&last_token) {
                        seen.insert(el)
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();
        }
    }

    (tier1, tier2, Vec::new())
}
