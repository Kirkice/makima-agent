use serde_json::{Map, Value, json};
use session::{
    ForkOptions, ForkPosition, JsonlSessionCreateOptions, JsonlSessionListOptions,
    JsonlSessionRepository, NewEntry,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMPORARY_DIRECTORY: AtomicU64 = AtomicU64::new(0);

fn temporary_directory(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "makima-session-repository-{name}-{}-{}",
        std::process::id(),
        NEXT_TEMPORARY_DIRECTORY.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn create_options(cwd: &str, id: &str) -> JsonlSessionCreateOptions {
    JsonlSessionCreateOptions {
        cwd: cwd.into(),
        id: Some(id.into()),
        parent_session_id: None,
        metadata: Some(Map::from_iter([(String::from("project"), json!("makima"))])),
    }
}

fn message(id: &str) -> NewEntry {
    NewEntry {
        lane: "main".into(),
        fields: Map::from_iter([
            (String::from("id"), Value::String(id.into())),
            (String::from("type"), Value::String("message".into())),
            (
                String::from("message"),
                json!({ "role": "user", "content": "hello" }),
            ),
        ]),
    }
}

#[test]
fn create_list_open_and_delete_preserve_metadata_and_cwd_filtering() {
    let root = temporary_directory("metadata");
    let repository = JsonlSessionRepository::new(&root).unwrap();
    let cwd_a = root.join("project-a").to_string_lossy().into_owned();
    let cwd_b = root.join("project-b").to_string_lossy().into_owned();

    let first = repository.create(create_options(&cwd_a, "one")).unwrap();
    let first_path = first.path().to_owned();
    drop(first);
    let second = repository.create(create_options(&cwd_b, "two")).unwrap();
    drop(second);

    let all = repository.list(JsonlSessionListOptions::default()).unwrap();
    assert_eq!(all.len(), 2);
    let filtered = repository
        .list(JsonlSessionListOptions {
            cwd: Some(cwd_a.clone()),
        })
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "one");
    assert_eq!(
        filtered[0].metadata.as_ref().unwrap().get("project"),
        Some(&json!("makima"))
    );

    let reopened = repository.open(&filtered[0]).unwrap();
    assert_eq!(reopened.path(), first_path);
    drop(reopened);
    repository.delete(&filtered[0]).unwrap();
    assert!(
        repository
            .list(JsonlSessionListOptions::default())
            .unwrap()
            .iter()
            .all(|item| item.id != "one")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_invalid_or_duplicate_ids_and_excludes_active_writer_from_second_open() {
    let root = temporary_directory("leases");
    let repository = JsonlSessionRepository::new(&root).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();
    assert!(
        repository
            .create(create_options(&cwd, "invalid/id"))
            .is_err()
    );

    let first = repository
        .create(create_options(&cwd, "stable-id"))
        .unwrap();
    let metadata = repository
        .list(JsonlSessionListOptions::default())
        .unwrap()
        .remove(0);
    assert!(
        repository
            .create(create_options(&cwd, "stable-id"))
            .is_err()
    );
    assert!(repository.open(&metadata).is_err());
    drop(first);
    let reopened = repository.open(&metadata).unwrap();
    drop(reopened);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn forks_branch_and_tree_with_typescript_lane_and_label_semantics() {
    let root = temporary_directory("fork");
    let repository = JsonlSessionRepository::new(&root).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();
    let mut source = repository.create(create_options(&cwd, "source")).unwrap();
    source.append_entry(message("first")).unwrap();
    source.append_entry(message("second")).unwrap();
    source.create_lane("review", Some("first")).unwrap();
    source.set_label("first", Some("keep")).unwrap();
    let source_metadata = repository
        .list(JsonlSessionListOptions::default())
        .unwrap()
        .remove(0);
    drop(source);

    let branch = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "branch"),
            ForkOptions::Branch {
                entry_id: Some("second".into()),
                position: Some(ForkPosition::Before),
            },
        )
        .unwrap();
    assert_eq!(branch.header().parent_session_id.as_deref(), Some("source"));
    assert_eq!(
        branch
            .state()
            .find_entries(Default::default())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(branch.state().lanes().len(), 1);
    assert_eq!(branch.state().lane_leaf("main"), Some(Some("first")));
    assert_eq!(branch.state().label("first"), Some("keep"));
    drop(branch);

    let tree = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "tree"),
            ForkOptions::Tree,
        )
        .unwrap();
    let lanes = tree.state().lanes();
    assert_eq!(
        lanes
            .iter()
            .map(|lane| lane.lane.as_str())
            .collect::<Vec<_>>(),
        vec!["main", "review"]
    );
    assert_eq!(
        tree.state().find_entries(Default::default()).unwrap().len(),
        2
    );

    // fork 返回后目标已发布，但最终路径的 writer lease 仍由返回句柄持有；
    // 这覆盖临时文件 rename 与返回句柄之间不能被另一 writer 抢占的窗口。
    let tree_metadata = repository
        .list(JsonlSessionListOptions::default())
        .unwrap()
        .into_iter()
        .find(|metadata| metadata.id == "tree")
        .unwrap();
    assert!(repository.open(&tree_metadata).is_err());
    drop(tree);
    let reopened_tree = repository.open(&tree_metadata).unwrap();
    drop(reopened_tree);
    fs::remove_dir_all(root).unwrap();
}
