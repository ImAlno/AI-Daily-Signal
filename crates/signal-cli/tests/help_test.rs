#[test]
fn documented_primary_commands_exist_in_help() {
    let output = assert_cmd::Command::cargo_bin("signal")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();
    for command in [
        "init", "refresh", "today", "latest", "show", "save", "saved", "status", "sources",
    ] {
        assert!(help.contains(command), "missing {command} from help");
    }
}
