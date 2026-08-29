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
