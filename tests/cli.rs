//! Runs the real binary with --dry-run and a throwaway config directory.

use std::path::Path;
use std::process::{Command, Output};

fn run(config_home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vagalume-rgb"))
        .env("XDG_CONFIG_HOME", config_home)
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn dry_run_prints_frames_and_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(
        dir.path(),
        &["--dry-run", "mode", "custom", "--color", "#0000ff"],
    );
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "would send: 3e 88 0f 00 27 0e\nwould send: 10 01\nwould send: 3e 88\n\
         would send: 1a 00 1b 00 1c ff\n"
    );
    assert!(!dir.path().join("vagalume-rgb").exists());
}

#[test]
fn dry_run_flag_also_works_after_the_command() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(dir.path(), &["led", "off", "--dry-run"])
        .status
        .success());
}

#[test]
fn rejects_out_of_range_setting() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(
        dir.path(),
        &["--dry-run", "mode", "stacking", "--set", "speed=99"],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("speed must be 2-13"));
}

#[test]
fn rejects_unknown_setting() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        !run(dir.path(), &["--dry-run", "mode", "custom", "--set", "x=1"])
            .status
            .success()
    );
}

#[test]
fn rejects_colour_on_colourless_mode() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!run(
        dir.path(),
        &["--dry-run", "mode", "chase", "--color", "#ff0000"]
    )
    .status
    .success());
}

#[test]
fn rejects_direction_on_directionless_mode() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!run(
        dir.path(),
        &["--dry-run", "mode", "custom", "--direction", "left"]
    )
    .status
    .success());
}

#[test]
fn monitor_sends_nothing_and_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(dir.path(), &["monitor"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("GPU temperature: "), "{text}");
    assert!(text.contains("VRAM temperature: "), "{text}");
    assert!(!text.contains("would send"), "{text}");
    assert!(!dir.path().join("vagalume-rgb").exists());
}

#[test]
fn monitor_can_show_fahrenheit() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(dir.path(), &["monitor", "--fahrenheit"]);
    assert!(output.status.success());
}

#[test]
fn list_names_every_effect() {
    let dir = tempfile::tempdir().unwrap();
    let text = stdout(&run(dir.path(), &["list"]));
    for key in [
        "rainbow",
        "chase",
        "gradient",
        "sequential",
        "stacking",
        "breathing",
        "custom",
    ] {
        assert!(text.contains(&format!("{key}: ")), "{key}");
    }
}

#[test]
fn profile_save_apply_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(dir.path(), &["profile", "save", "Mine"])
        .status
        .success());
    assert!(stdout(&run(dir.path(), &["profile", "list"])).contains("Mine: Rainbow"));

    let applied = stdout(&run(
        dir.path(),
        &["--dry-run", "profile", "apply", "Night"],
    ));
    assert!(
        applied.contains("would send: 1a ff 1b 6a 1c 20"),
        "{applied}"
    );

    assert!(run(dir.path(), &["profile", "delete", "Mine"])
        .status
        .success());
    assert!(!stdout(&run(dir.path(), &["profile", "list"])).contains("Mine"));
    assert!(!run(dir.path(), &["profile", "apply", "Mine", "--dry-run"])
        .status
        .success());
}

#[test]
fn profile_dry_run_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(dir.path(), &["--dry-run", "profile", "save", "Mine"])
        .status
        .success());
    assert!(!dir.path().join("vagalume-rgb").exists());
}
