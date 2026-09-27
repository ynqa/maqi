use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_man2usage"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn requires_exactly_one_input_source() {
    for args in [
        vec![],
        vec![
            "--bin",
            "example",
            "--debian-man-url",
            "https://manpages.debian.org/example",
        ],
    ] {
        let output = run(&args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
    let output = run(&["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--bin") && help.contains("--debian-man-url"));
    for removed in ["--input ", "--format", "--follow", "--url-template"] {
        assert!(!help.contains(removed));
    }
}

#[test]
fn installed_man_to_kdl_and_missing_man_preserves_output() {
    let temp = tempfile::tempdir().unwrap();
    let man1 = temp.path().join("man1");
    fs::create_dir(&man1).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/example.1");
    fs::write(
        man1.join("example.1"),
        format!(
            "{}\n.SH SEE ALSO\nexample-child(1)\n",
            fs::read_to_string(fixture).unwrap()
        ),
    )
    .unwrap();
    fs::write(man1.join("example-child.1"), ".TH EXAMPLE 1\n.SH NAME\nexample child - inspect a child\n.SH SYNOPSIS\nexample child [OPTIONS]\n.SH OPTIONS\n.PP\n\\fB--verbose\\fP=false\n\tPrint details.\n").unwrap();
    // An unrelated manual must not enter the command tree.
    fs::write(man1.join("example-unrelated.1"), "not a manual").unwrap();
    let output = temp.path().join("example.kdl");
    let generate = |bin| {
        Command::new(env!("CARGO_BIN_EXE_man2usage"))
            .args(["--bin", bin, "--output", output.to_str().unwrap()])
            .env("MANPATH", temp.path())
            .env("MANSECT", "1")
            .env_remove("MANOPT")
            .output()
            .unwrap()
    };
    let result = generate("example");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let kdl = fs::read_to_string(&output).unwrap();
    let spec: usage::Spec = kdl.parse().unwrap();
    assert_eq!(spec.cmd.flags.len(), 3);
    assert_eq!(spec.cmd.subcommands.len(), 1);
    assert_eq!(spec.cmd.subcommands["child"].flags[0].long, ["verbose"]);
    let missing = generate("man2usage-no-such-command");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("no installed man page"));
    assert_eq!(fs::read_to_string(output).unwrap(), kdl);
}

#[test]
fn rejects_non_debian_urls_before_network_or_output() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("result.kdl");
    fs::write(&path, "existing").unwrap();
    let output = run(&[
        "--debian-man-url",
        "https://example.org/tool.1.html",
        "--output",
        path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("manpages.debian.org"));
    assert_eq!(fs::read_to_string(path).unwrap(), "existing");
}
