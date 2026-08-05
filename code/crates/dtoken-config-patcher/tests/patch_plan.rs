use dtoken_config_patcher::{
    mock_patch_plan, simple_diff, stable_hash, ConfigPatcher, ExpectedFileState,
};
use dtoken_types::{ConfigToolKind, RiskLevel};
use std::{fs, path::Path};
use tempfile::TempDir;

fn workspace_tempdir() -> TempDir {
    tempfile::tempdir_in(std::env::current_dir().expect("current directory"))
        .expect("temporary directory")
}

fn prepare_json(
    patcher: &ConfigPatcher,
    target: &Path,
    replacement: &str,
) -> dtoken_config_patcher::PreparedChange {
    patcher
        .prepare_change(
            target,
            replacement,
            ConfigToolKind::Dtoken,
            None,
            RiskLevel::Low,
        )
        .expect("prepared configuration change")
}

#[test]
fn stable_hash_is_sha256() {
    assert_eq!(
        stable_hash("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn diff_is_unified_and_redacts_sensitive_values() {
    let diff = simple_diff(
        r#"{"api_key":"old-secret","enabled":false}"#,
        r#"{"api_key":"new-secret","enabled":true}"#,
    );

    assert!(diff.contains("--- before"));
    assert!(diff.contains("+++ after"));
    assert!(diff.contains("\"enabled\": false"));
    assert!(diff.contains("\"enabled\": true"));
    assert!(diff.contains("<redacted>"));
    assert!(!diff.contains("old-secret"));
    assert!(!diff.contains("new-secret"));
}

#[test]
fn plain_text_diff_redacts_sensitive_lines() {
    let diff = simple_diff("token=first\nmode=off", "token=second\nmode=on");

    assert!(diff.contains("mode=off"));
    assert!(diff.contains("mode=on"));
    assert!(!diff.contains("first"));
    assert!(!diff.contains("second"));
}

#[test]
fn diff_redacts_unlabeled_and_provider_labeled_credentials() {
    let github = "ghp_abcdefghijklmnopqrstuvwxyz012345";
    let slack = "xoxb-123456789-abcdefghijklmnopqrstuv";
    let json_diff = simple_diff(
        &format!(r#"{{"note":"{github}","mode":"off"}}"#),
        &format!(r#"{{"github_token":"{github}","mode":"on"}}"#),
    );
    assert!(!json_diff.contains(github));
    assert!(json_diff.contains("<redacted>"));

    let text_diff = simple_diff(
        &format!("endpoint=local\nvalue={slack}"),
        &format!("endpoint=remote\nvalue={slack}"),
    );
    assert!(!text_diff.contains(slack));
    assert!(text_diff.contains("<redacted sensitive setting>"));
}

#[test]
fn missing_target_can_be_planned_applied_and_removed_by_restore() {
    let root = workspace_tempdir();
    let target = root.path().join("new.json");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let change = prepare_json(&patcher, &target, r#"{"enabled":true}"#);

    assert!(!change.before.exists);
    assert_eq!(change.plan.before_hash, stable_hash(""));
    let backup = patcher.backup_prepared(&change).expect("missing backup");
    assert!(!backup.original.exists);
    assert!(backup.content_file.is_none());
    assert!(!backup.backup_dir.join("original.bin").exists());
    let loaded = patcher
        .load_backup(&backup.rollback_id)
        .expect("missing-file manifest without original.bin is valid");
    assert!(!loaded.original.exists);

    let receipt = patcher
        .apply_prepared(&change, &backup)
        .expect("apply new target");
    assert_eq!(
        fs::read_to_string(&target).expect("new target content"),
        r#"{"enabled":true}"#
    );
    let restore = patcher
        .restore_backup(
            &receipt.backup,
            &ExpectedFileState::Hash(change.plan.after_hash.clone()),
        )
        .expect("remove new target during restore");
    assert!(!restore.restored.exists);
    assert!(!target.exists());
}

#[test]
fn existing_target_backup_without_original_bytes_fails_closed() {
    let root = workspace_tempdir();
    let target = root.path().join("config.json");
    fs::write(&target, r#"{"enabled":false}"#).expect("fixture");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let change = prepare_json(&patcher, &target, r#"{"enabled":true}"#);
    let backup = patcher.backup_prepared(&change).expect("backup");
    assert!(backup.original.exists);
    fs::remove_file(backup.backup_dir.join("original.bin")).expect("remove backup bytes");

    patcher
        .load_backup(&backup.rollback_id)
        .expect_err("existing-file manifest without original.bin must fail closed");
}

#[test]
fn directory_target_is_rejected_instead_of_treated_as_empty() {
    let root = workspace_tempdir();
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let error = patcher
        .plan_replace(
            root.path(),
            "{}",
            ConfigToolKind::Dtoken,
            None,
            RiskLevel::Low,
        )
        .expect_err("directory target must fail");

    assert!(error.to_string().contains("regular file"));
}

#[test]
fn invalid_or_duplicate_key_json_is_rejected_recursively() {
    let root = workspace_tempdir();
    let target = root.path().join("config.json");
    fs::write(&target, "{}").expect("fixture");
    let patcher = ConfigPatcher::new(root.path().join("backups"));

    let duplicate_error = patcher
        .plan_replace(
            &target,
            r#"{"outer":{"route":1,"route":2}}"#,
            ConfigToolKind::Dtoken,
            None,
            RiskLevel::Low,
        )
        .expect_err("duplicate key must fail");
    assert!(format!("{duplicate_error:#}").contains("duplicate object key"));

    patcher
        .plan_replace(
            &target,
            "{not-json}",
            ConfigToolKind::Dtoken,
            None,
            RiskLevel::Low,
        )
        .expect_err("invalid JSON must fail");

    fs::write(&target, r#"{"route":1,"route":2}"#).expect("duplicate existing fixture");
    patcher
        .plan_replace(&target, "{}", ConfigToolKind::Dtoken, None, RiskLevel::Low)
        .expect_err("duplicate existing JSON must fail");
}

#[test]
fn stale_plan_does_not_overwrite_external_change() {
    let root = workspace_tempdir();
    let target = root.path().join("config.json");
    fs::write(&target, r#"{"value":"original"}"#).expect("fixture");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let replacement = r#"{"value":"planned-secret"}"#;
    let plan = patcher
        .plan_replace(
            &target,
            replacement,
            ConfigToolKind::Dtoken,
            None,
            RiskLevel::Low,
        )
        .expect("plan");
    fs::write(&target, r#"{"value":"external"}"#).expect("external change");

    let error = patcher
        .apply_replace(&plan, replacement)
        .expect_err("stale apply must fail");
    assert!(format!("{error:#}").contains("stale patch plan"));
    assert!(!format!("{error:#}").contains("planned-secret"));
    assert_eq!(
        fs::read_to_string(target).expect("preserved external content"),
        r#"{"value":"external"}"#
    );
}

#[test]
fn restore_refuses_to_overwrite_a_post_apply_external_change() {
    let root = workspace_tempdir();
    let target = root.path().join("config.json");
    fs::write(&target, r#"{"value":"original"}"#).expect("fixture");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let change = prepare_json(&patcher, &target, r#"{"value":"applied"}"#);
    let backup = patcher.backup_prepared(&change).expect("backup");
    let receipt = patcher.apply_prepared(&change, &backup).expect("apply");
    fs::write(&target, r#"{"value":"external"}"#).expect("external edit");

    patcher
        .restore_backup(
            &receipt.backup,
            &ExpectedFileState::Hash(change.plan.after_hash.clone()),
        )
        .expect_err("restore must reject changed current content");
    assert_eq!(
        fs::read_to_string(target).expect("external content preserved"),
        r#"{"value":"external"}"#
    );
}

#[test]
fn backup_records_exact_bytes_existence_hash_and_timestamp() {
    let root = workspace_tempdir();
    let target = root.path().join("config.json");
    let original = b"{\n  \"enabled\": false\n}\n";
    fs::write(&target, original).expect("fixture");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let change = prepare_json(&patcher, &target, r#"{"enabled":true}"#);
    let backup = patcher.backup_prepared(&change).expect("backup");

    assert!(backup.original.exists);
    assert_eq!(backup.original.byte_len, original.len() as u64);
    assert_eq!(
        backup.original.sha256.as_deref(),
        Some(stable_hash(std::str::from_utf8(original).expect("UTF-8")).as_str())
    );
    assert_ne!(backup.created_at, "m1-placeholder");
    assert_eq!(
        fs::read(backup.backup_dir.join("original.bin")).expect("backup bytes"),
        original
    );
    assert!(backup.backup_dir.join("manifest.json").is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&backup.backup_dir)
                .expect("backup directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        for file_name in ["original.bin", "manifest.json"] {
            assert_eq!(
                fs::metadata(backup.backup_dir.join(file_name))
                    .expect("private backup file metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn apply_and_restore_preserve_exact_unix_mode_and_leave_no_temp_file() {
    use std::os::unix::fs::PermissionsExt;

    let root = workspace_tempdir();
    let target = root.path().join("config.json");
    let original = "{\n  \"enabled\": false\n}\n";
    fs::write(&target, original).expect("fixture");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).expect("fixture mode");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let change = prepare_json(&patcher, &target, r#"{"enabled":true}"#);
    let backup = patcher.backup_prepared(&change).expect("backup");
    let receipt = patcher
        .apply_prepared(&change, &backup)
        .expect("atomic apply");

    assert_eq!(
        fs::metadata(&target)
            .expect("applied metadata")
            .permissions()
            .mode()
            & 0o7777,
        0o640
    );
    assert!(!fs::read_dir(root.path())
        .expect("root entries")
        .flatten()
        .any(|entry| entry.file_name().to_string_lossy().starts_with(".dtoken-")));

    patcher
        .restore_backup(
            &receipt.backup,
            &ExpectedFileState::Hash(change.plan.after_hash.clone()),
        )
        .expect("restore");
    assert_eq!(
        fs::read_to_string(&target).expect("restored bytes"),
        original
    );
    assert_eq!(
        fs::metadata(target)
            .expect("restored metadata")
            .permissions()
            .mode()
            & 0o7777,
        0o640
    );
}

#[cfg(unix)]
#[test]
fn permission_change_after_backup_invalidates_prepared_change() {
    use std::os::unix::fs::PermissionsExt;

    let root = workspace_tempdir();
    let target = root.path().join("config.json");
    fs::write(&target, "{}").expect("fixture");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).expect("initial mode");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let change = prepare_json(&patcher, &target, r#"{"enabled":true}"#);
    let backup = patcher.backup_prepared(&change).expect("backup");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).expect("external chmod");

    patcher
        .apply_prepared(&change, &backup)
        .expect_err("metadata-stale change must fail");
    assert_eq!(fs::read_to_string(target).expect("unchanged bytes"), "{}");
}

#[test]
fn changed_manifest_or_backup_content_is_rejected() {
    let root = workspace_tempdir();
    let target = root.path().join("config.json");
    fs::write(&target, r#"{"enabled":false}"#).expect("fixture");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let change = prepare_json(&patcher, &target, r#"{"enabled":true}"#);
    let backup = patcher.backup_prepared(&change).expect("backup");
    let receipt = patcher.apply_prepared(&change, &backup).expect("apply");

    let manifest_path = backup.backup_dir.join("manifest.json");
    let mut manifest = fs::read(&manifest_path).expect("manifest");
    manifest.push(b' ');
    fs::write(&manifest_path, manifest).expect("tamper manifest");
    patcher
        .restore_backup(
            &receipt.backup,
            &ExpectedFileState::Hash(change.plan.after_hash.clone()),
        )
        .expect_err("changed manifest must fail");

    let second_root = workspace_tempdir();
    let second_target = second_root.path().join("config.json");
    fs::write(&second_target, r#"{"enabled":false}"#).expect("second fixture");
    let second_patcher = ConfigPatcher::new(second_root.path().join("backups"));
    let second_change = prepare_json(&second_patcher, &second_target, "{}");
    let second_backup = second_patcher
        .backup_prepared(&second_change)
        .expect("second backup");
    fs::write(second_backup.backup_dir.join("original.bin"), b"{}").expect("tamper content");
    second_patcher
        .load_backup(&second_backup.rollback_id)
        .expect_err("changed backup content must fail");
}

#[test]
fn multi_target_transaction_applies_all_changes() {
    let root = workspace_tempdir();
    let first = root.path().join("first.json");
    let second = root.path().join("second.json");
    fs::write(&first, r#"{"value":1}"#).expect("first fixture");
    fs::write(&second, r#"{"value":2}"#).expect("second fixture");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let changes = [
        prepare_json(&patcher, &first, r#"{"value":10}"#),
        prepare_json(&patcher, &second, r#"{"value":20}"#),
    ];

    let receipt = patcher
        .apply_transaction(&changes)
        .expect("transaction apply");
    assert_eq!(receipt.changes.len(), 2);
    assert_eq!(
        fs::read_to_string(first).expect("first result"),
        r#"{"value":10}"#
    );
    assert_eq!(
        fs::read_to_string(second).expect("second result"),
        r#"{"value":20}"#
    );
}

#[test]
fn duplicate_transaction_target_is_rejected_before_writes() {
    let root = workspace_tempdir();
    let target = root.path().join("config.json");
    fs::write(&target, "{}").expect("fixture");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    let first = prepare_json(&patcher, &target, r#"{"value":1}"#);
    let second = prepare_json(&patcher, &target, r#"{"value":2}"#);

    patcher
        .apply_transaction(&[first, second])
        .expect_err("duplicate target must fail");
    assert_eq!(fs::read_to_string(target).expect("unchanged target"), "{}");
}

#[cfg(unix)]
#[test]
fn target_or_parent_symlink_is_rejected() {
    use std::os::unix::fs::symlink;

    let root = workspace_tempdir();
    let real = root.path().join("real.json");
    let linked = root.path().join("linked.json");
    fs::write(&real, "{}").expect("real fixture");
    symlink(&real, &linked).expect("target symlink");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    patcher
        .plan_replace(&linked, "{}", ConfigToolKind::Dtoken, None, RiskLevel::Low)
        .expect_err("target symlink must fail");

    let real_parent = root.path().join("real-parent");
    let linked_parent = root.path().join("linked-parent");
    fs::create_dir(&real_parent).expect("real parent");
    symlink(&real_parent, &linked_parent).expect("parent symlink");
    patcher
        .plan_replace(
            linked_parent.join("config.json"),
            "{}",
            ConfigToolKind::Dtoken,
            None,
            RiskLevel::Low,
        )
        .expect_err("parent symlink must fail");
}

#[test]
fn parent_directory_components_are_rejected() {
    let root = workspace_tempdir();
    let target = root.path().join("child").join("..").join("config.json");
    let patcher = ConfigPatcher::new(root.path().join("backups"));
    patcher
        .plan_replace(target, "{}", ConfigToolKind::Dtoken, None, RiskLevel::Low)
        .expect_err("parent component must fail");
}

#[test]
fn mock_patch_plan_has_safe_defaults() {
    let plan = mock_patch_plan();
    assert_eq!(plan.tool, ConfigToolKind::Dtoken);
    assert_eq!(plan.risk_level, RiskLevel::Low);
    assert_eq!(plan.before_hash.len(), 64);
}
