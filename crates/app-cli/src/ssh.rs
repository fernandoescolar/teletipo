use std::fs;

/// An SSH host alias parsed from `~/.ssh/config`.
#[derive(Clone, Debug)]
pub(crate) struct SshHost {
    /// The `Host` alias (e.g. `myserver`). Wildcards are excluded.
    pub(crate) name: String,
    /// Optional `HostName` override.
    pub(crate) hostname: Option<String>,
    /// Optional `User` field.
    pub(crate) user: Option<String>,
    /// Optional `Port` field.
    pub(crate) port: Option<u16>,
}

impl SshHost {
    /// Build the `ssh` command string for this host.
    pub(crate) fn ssh_command(&self) -> String {
        let mut args = String::from("ssh");
        if let Some(ref user) = self.user {
            args.push_str(&format!(" -l {user}"));
        }
        if let Some(port) = self.port {
            args.push_str(&format!(" -p {port}"));
        }
        args.push(' ');
        args.push_str(&self.name);
        args
    }
}

/// Parse `~/.ssh/config` and return all concrete `Host` entries (no wildcards).
pub(crate) fn load_ssh_hosts() -> Vec<SshHost> {
    let path = match dirs::home_dir() {
        Some(home) => home.join(".ssh").join("config"),
        None => return vec![],
    };
    let content = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    parse_ssh_config(&content)
}

fn parse_ssh_config(content: &str) -> Vec<SshHost> {
    let mut hosts: Vec<SshHost> = Vec::new();
    let mut current: Option<SshHost> = None;

    for line in content.lines() {
        let line = line.trim();
        // Skip comments and empty lines.
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Split on whitespace or `=`.
        let (key, value) = match line.split_once([' ', '\t', '=']) {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };

        if key.eq_ignore_ascii_case("Host") {
            // Flush the previous stanza.
            if let Some(host) = current.take() {
                hosts.push(host);
            }
            // Skip wildcard patterns.
            if value.contains('*') || value.contains('?') || value.contains('!') {
                continue;
            }
            // `Host` can list multiple patterns space-separated; take only single-name entries.
            let names: Vec<&str> = value.split_whitespace().collect();
            if names.len() == 1 {
                current = Some(SshHost {
                    name: names[0].to_owned(),
                    hostname: None,
                    user: None,
                    port: None,
                });
            }
        } else if let Some(ref mut host) = current {
            if key.eq_ignore_ascii_case("HostName") {
                host.hostname = Some(value.to_owned());
            } else if key.eq_ignore_ascii_case("User") {
                host.user = Some(value.to_owned());
            } else if key.eq_ignore_ascii_case("Port") {
                host.port = value.parse().ok();
            }
        }
    }
    if let Some(host) = current.take() {
        hosts.push(host);
    }
    hosts
}

#[cfg(test)]
mod tests {
    use super::parse_ssh_config;

    #[test]
    fn parses_basic_hosts() {
        let cfg = "
Host myserver
    HostName example.com
    User admin
    Port 2222

Host prod
    HostName prod.example.com
";
        let hosts = parse_ssh_config(cfg);
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].name, "myserver");
        assert_eq!(hosts[0].hostname.as_deref(), Some("example.com"));
        assert_eq!(hosts[0].user.as_deref(), Some("admin"));
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(hosts[1].name, "prod");
    }

    #[test]
    fn skips_wildcards() {
        let cfg = "
Host *
    ServerAliveInterval 60

Host myhost
    HostName myhost.example.com
";
        let hosts = parse_ssh_config(cfg);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "myhost");
    }

    #[test]
    fn ssh_command_with_user_and_port() {
        let hosts = parse_ssh_config("Host dev\n    User bob\n    Port 22\n");
        assert_eq!(hosts[0].ssh_command(), "ssh -l bob -p 22 dev");
    }

    #[test]
    fn ssh_command_plain() {
        let hosts = parse_ssh_config("Host simple\n");
        assert_eq!(hosts[0].ssh_command(), "ssh simple");
    }
}
