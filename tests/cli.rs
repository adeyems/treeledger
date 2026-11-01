use std::{fs, process::Command};

#[test]
fn cli_roundtrip_exit_codes_and_invalid_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("files");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("demo.txt"), b"synthetic one").unwrap();
    let a = temp.path().join("before.json");
    let b = temp.path().join("after.json");
    let exe = env!("CARGO_BIN_EXE_treeledger");
    assert!(
        Command::new(exe)
            .arg("scan")
            .arg(&root)
            .arg(&a)
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(root.join("demo.txt"), b"synthetic two").unwrap();
    assert!(
        Command::new(exe)
            .arg("scan")
            .arg(&root)
            .arg(&b)
            .output()
            .unwrap()
            .status
            .success()
    );
    let changed = Command::new(exe)
        .arg("compare")
        .arg(&a)
        .arg(&b)
        .output()
        .unwrap();
    assert_eq!(changed.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&changed.stdout).unwrap();
    assert_eq!(report["changes"][0]["kind"], "changed");
    assert!(changed.stderr.is_empty());
    assert!(
        Command::new(exe)
            .arg("compare")
            .arg(&a)
            .arg(&a)
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(&b, b"{}").unwrap();
    let invalid = Command::new(exe)
        .arg("compare")
        .arg(&a)
        .arg(&b)
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    assert!(
        serde_json::from_slice::<serde_json::Value>(&invalid.stderr)
            .unwrap()
            .get("error")
            .is_some()
    );
}

#[cfg(unix)]
#[test]
fn unreadable_file_marks_scan_incomplete_when_permissions_enforced() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("files");
    fs::create_dir(&root).unwrap();
    let file = root.join("denied");
    fs::write(&file, b"synthetic").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o0)).unwrap();
    let enforced = fs::File::open(&file).is_err();
    let output = temp.path().join("scan.json");
    let run = Command::new(env!("CARGO_BIN_EXE_treeledger"))
        .arg("scan")
        .arg(&root)
        .arg(&output)
        .output()
        .unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    if enforced {
        assert_eq!(run.status.code(), Some(1));
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert_eq!(manifest["complete"], false);
        assert_eq!(
            manifest["issues"][0]["reason"],
            "unreadable_or_changed_file"
        );
    } else {
        eprintln!("Permission check unavailable for privileged user");
    }
}

#[test]
fn cli_budget_and_existing_output_are_portable() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("files");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.txt"), b"a").unwrap();
    fs::write(root.join("b.txt"), b"b").unwrap();
    let output = temp.path().join("limited.json");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_treeledger"))
            .arg("scan")
            .arg(&root)
            .arg(&output)
            .args(["--max-entries", "1"])
            .output()
            .unwrap()
    };
    assert_eq!(invoke().status.code(), Some(1));
    let bytes = fs::read(&output).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(manifest["complete"], false);
    assert_eq!(manifest["issues"][0]["reason"], "entry_limit");
    assert_eq!(invoke().status.code(), Some(2));
    assert_eq!(fs::read(&output).unwrap(), bytes);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
}
