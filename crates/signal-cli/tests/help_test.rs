use std::collections::BTreeSet;

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
            [
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
            ]
            .as_slice(),
        ),
        (
            &["summarize", "--help"][..],
            ["--profile", "--force"].as_slice(),
        ),
        (&["refresh", "--help"][..], ["--no-ai"].as_slice()),
    ];

    for (arguments, expected_flags) in cases {
        let output = assert_cmd::Command::cargo_bin("signal")
            .unwrap()
            .args(arguments)
            .output()
            .unwrap();
        assert!(output.status.success(), "help failed for {arguments:?}");
        let help = String::from_utf8(output.stdout).unwrap();
        let actual = command_specific_long_options(&help);
        let expected = expected_flags
            .iter()
            .map(|flag| (*flag).to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected, "wrong option set for {arguments:?}");
        for lookalike in ["--model-id", "--forceful", "--no-ai-mode"] {
            assert!(
                !actual.contains(lookalike),
                "lookalike {lookalike} appeared in help for {arguments:?}"
            );
        }
    }
}

fn command_specific_long_options(help: &str) -> BTreeSet<String> {
    help.split_whitespace()
        .filter_map(|token| {
            let token = token.trim_matches(|character| matches!(character, ',' | '[' | ']'));
            let option = token.split(['=', '<']).next().unwrap_or(token);
            option.starts_with("--").then(|| option.to_owned())
        })
        .filter(|option| !matches!(option.as_str(), "--json" | "--plain" | "--help"))
        .collect()
}
