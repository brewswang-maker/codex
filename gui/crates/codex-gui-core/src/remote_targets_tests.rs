//! Unit coverage of the ssh-config parsing behind the remote pane.
#![allow(clippy::expect_used)]

use super::parse_ssh_config;

#[test]
fn host_stanzas_become_targets_with_patterns_dropped() {
    let config = "\
Host github.com
    User git

Host 56mf82ae9539.vicp.fun web-*
    Port 22022

# a commented Host line stays invisible
Hostname ignored.example

host build-server
";
    let hosts: Vec<String> = parse_ssh_config(config)
        .into_iter()
        .map(|target| target.host)
        .collect();
    assert_eq!(
        hosts,
        vec!["github.com", "56mf82ae9539.vicp.fun", "build-server"]
    );
}

#[test]
fn quoted_aliases_and_blank_input_survive() {
    assert_eq!(
        parse_ssh_config("Host \"dev box\"\n"),
        vec![super::SshTarget {
            host: String::from("dev box")
        }]
    );
    assert!(parse_ssh_config("").is_empty());
    assert!(parse_ssh_config("Port 22\nUser git\n").is_empty());
}
