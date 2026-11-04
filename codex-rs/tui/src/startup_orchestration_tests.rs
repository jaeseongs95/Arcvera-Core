#[test]
fn automatic_daemon_requires_the_full_known_cli_version() {
    let cli = "0.166.0+native-r37.20261104";
    for server in [Some(cli), Some("0.166.0+native-r28.20261029"), None] {
        let warning = super::automatic_daemon_version_warning(cli, server);
        assert_eq!(warning.is_some(), server != Some(cli));
        if let Some(warning) = warning {
            assert!(warning.contains(cli));
            let unknown = crate::i18n::message("status-unknown-version", &[]);
            assert!(warning.contains(server.unwrap_or(&unknown)));
        }
    }
}
