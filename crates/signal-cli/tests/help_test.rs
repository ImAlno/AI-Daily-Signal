#[test]
fn documented_primary_commands_exist_in_help() {
    let output = assert_cmd::Command::cargo_bin("signal")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();
    for command in [
        "init",
        "refresh",
        "today",
        "latest",
        "show",
        "save",
        "saved",
        "status",
        "sources",
        "models",
        "summarize",
    ] {
        assert!(help.contains(command), "missing {command} from help");
    }
}

#[test]
fn model_subcommands_exist_in_help() {
    let output = assert_cmd::Command::cargo_bin("signal")
        .unwrap()
        .args(["models", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();
    for command in ["list", "add", "use", "test", "remove"] {
        assert!(help.contains(command), "missing {command} from models help");
    }
}

#[test]
fn nested_generation_help_exposes_every_supported_flag() {
    let cases = [
        (
            &["models", "add", "--help"][..],
            &[
                "--name",
                "--provider",
                "--model",
                "--endpoint",
                "--dialect",
                "--credential-env",
                "--max-summaries",
                "--daily-budget-usd",
                "--input-usd-per-million",
                "--output-usd-per-million",
                "--max-output-tokens",
                "--timeout-seconds",
                "--max-retries",
                "--consent-provider-data-sharing",
            ][..],
        ),
        (&["summarize", "--help"][..], &["--model", "--force"][..]),
        (&["refresh", "--help"][..], &["--no-ai"][..]),
    ];

    for (arguments, expected_flags) in cases {
        let output = assert_cmd::Command::cargo_bin("signal")
            .unwrap()
            .args(arguments)
            .output()
            .unwrap();
        assert!(output.status.success(), "help failed for {arguments:?}");
        let help = String::from_utf8(output.stdout).unwrap();
        for flag in expected_flags {
            assert!(
                help.contains(flag),
                "missing {flag} from help for {arguments:?}"
            );
        }
    }
}
