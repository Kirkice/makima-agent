use serde_json::{Map, Value, json};
use session::{
    EntryQuery, ForkOptions, ForkPosition, JsonlSessionCreateOptions, JsonlSessionListOptions,
    JsonlSessionRepository, NewEntry, SessionErrorCode, SessionFilePublisher,
    StandardSessionFilePublisher,
};
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

static NEXT_TEMPORARY_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const FAIL_NONE: u8 = 0;
const FAIL_CREATE_WRITE: u8 = 1;
const FAIL_APPEND: u8 = 2;
const FAIL_RENAME: u8 = 3;
const FAIL_CREATE_AFTER_COMPETITOR_PUBLISH: u8 = 4;

/// 单次故障发布器只替换目标 I/O 原语，其余调用委托真实文件系统。
///
/// 故障通过原子状态“消费”一次，因此失败后的同目标重试仍走完全相同的
/// repository 实现，可同时验证 create claim 被释放且暂存文件已清理。
#[derive(Debug, Default)]
struct FailingSessionFilePublisher {
    failure: AtomicU8,
    standard: StandardSessionFilePublisher,
}

impl FailingSessionFilePublisher {
    fn fail_once(&self, failure: u8) {
        self.failure.store(failure, Ordering::SeqCst);
    }

    fn consume(&self, failure: u8) -> bool {
        self.failure
            .compare_exchange(failure, FAIL_NONE, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

impl SessionFilePublisher for FailingSessionFilePublisher {
    fn create_new(&self, path: &Path) -> io::Result<File> {
        if self.consume(FAIL_CREATE_WRITE) {
            // 先真实创建目标，再返回只读句柄，使 header 的首次 write 稳定失败。
            // 这比让 create-new 直接失败覆盖更多路径：Store 必须删除已创建的
            // partial 文件，否则 repository 的后续同 ID 重试会被永久阻塞。
            drop(self.standard.create_new(path)?);
            return File::open(path);
        }
        if self.consume(FAIL_CREATE_AFTER_COMPETITOR_PUBLISH) {
            // 模拟 exists 检查后，另一创建者抢先发布同一路径。当前调用没有取得
            // 文件所有权，因此 create-new 的失败清理绝不能删除竞争者的内容。
            fs::write(path, b"competitor session\n")?;
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "injected competing session publication",
            ));
        }
        self.standard.create_new(path)
    }

    fn append(&self, path: &Path) -> io::Result<File> {
        if self.consume(FAIL_APPEND) {
            return Err(io::Error::other("injected session append failure"));
        }
        self.standard.append(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.consume(FAIL_RENAME) {
            return Err(io::Error::other("injected session rename failure"));
        }
        self.standard.rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.standard.remove_file(path)
    }
}

fn temporary_files(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            result.extend(temporary_files(&path));
        } else if path.extension().and_then(|value| value.to_str()) == Some("tmp") {
            result.push(path);
        }
    }
    result
}

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
fn lists_complete_metadata_and_skips_malformed_headers_without_rewriting_them() {
    let root = temporary_directory("metadata-validation");
    let repository = JsonlSessionRepository::new(&root).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();
    let mut options = create_options(&cwd, "complete-metadata");
    options.parent_session_id = Some("parent-session".into());
    options.metadata = Some(Map::from_iter([
        (String::from("owner"), json!("agent")),
        (String::from("nested"), json!({ "enabled": true })),
    ]));
    let session = repository.create(options).unwrap();
    let valid_path = session.path().to_owned();
    drop(session);

    let malformed = repository
        .create(create_options(&cwd, "malformed-header"))
        .unwrap();
    let malformed_path = malformed.path().to_owned();
    drop(malformed);
    let malformed_contents = "not json\n";
    fs::write(&malformed_path, malformed_contents).unwrap();

    // `list()` 仅把可解码 v4 header 暴露给调用方；但不会擅自修复或改写损坏文件。
    let listed = repository
        .list(JsonlSessionListOptions {
            cwd: Some(cwd.clone()),
        })
        .unwrap();
    assert_eq!(listed.len(), 1);
    let metadata = &listed[0];
    assert_eq!(metadata.id, "complete-metadata");
    assert_eq!(metadata.cwd, cwd);
    assert_eq!(metadata.path, valid_path);
    assert_eq!(
        metadata.parent_session_id.as_deref(),
        Some("parent-session")
    );
    assert_eq!(metadata.source_format, 4);
    assert_eq!(
        metadata
            .metadata
            .as_ref()
            .and_then(|value| value.get("nested")),
        Some(&json!({ "enabled": true }))
    );
    assert!(metadata.created_at > 0);
    assert!(metadata.modified_at > 0);

    let malformed_metadata = session::JsonlSessionMetadata {
        id: "malformed-header".into(),
        created_at: 0,
        cwd,
        path: malformed_path.clone(),
        modified_at: 0,
        parent_session_id: None,
        legacy_parent_session_path: None,
        metadata: None,
        source_format: 4,
    };
    assert!(repository.open(&malformed_metadata).is_err());
    assert_eq!(
        fs::read_to_string(&malformed_path).unwrap(),
        malformed_contents
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lists_sessions_by_current_filesystem_modification_time() {
    let root = temporary_directory("modification-order");
    let repository = JsonlSessionRepository::new(&root).unwrap();
    let newest_cwd = root.join("newest").to_string_lossy().into_owned();
    let oldest_cwd = root.join("oldest").to_string_lossy().into_owned();
    let newest = repository
        .create(create_options(&newest_cwd, "newest"))
        .unwrap();
    let newest_path = newest.path().to_owned();
    drop(newest);
    let oldest = repository
        .create(create_options(&oldest_cwd, "oldest"))
        .unwrap();
    let oldest_path = oldest.path().to_owned();
    drop(oldest);

    // 列表顺序取当前文件系统 mtime，而非 header.createdAt 或文件名。手工调整
    // 时间后必须立即重排，才能与 TypeScript `listDirect()` 保持一致。
    let epoch = std::time::UNIX_EPOCH;
    OpenOptions::new()
        .write(true)
        .open(&newest_path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(epoch + std::time::Duration::from_secs(2)))
        .unwrap();
    OpenOptions::new()
        .write(true)
        .open(&oldest_path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(epoch + std::time::Duration::from_secs(1)))
        .unwrap();
    let listed = repository.list(JsonlSessionListOptions::default()).unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|metadata| metadata.id.as_str())
            .collect::<Vec<_>>(),
        vec!["newest", "oldest"]
    );
    assert_eq!(listed[0].modified_at, 2_000);
    assert_eq!(listed[1].modified_at, 1_000);
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
fn claims_one_destination_for_concurrent_create_and_fork_calls() {
    #[derive(Clone, Copy)]
    enum CreationKind {
        Create,
        Fork,
    }

    let root = temporary_directory("destination-claims");
    let repository = JsonlSessionRepository::new(&root).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();
    let source = repository.create(create_options(&cwd, "source")).unwrap();
    let source_metadata = repository
        .list(JsonlSessionListOptions::default())
        .unwrap()
        .into_iter()
        .find(|metadata| metadata.id == "source")
        .unwrap();
    drop(source);

    // TypeScript 使用进程内 reservation 防止 create/fork 在同一 `{cwd, id}`
    // 同时发布。Rust 的 sidecar create lock 需要覆盖三个组合，确保跨线程时
    // 只能有一个调用成功，另一个稳定报告 already_exists。
    for (index, (first_kind, second_kind)) in [
        (CreationKind::Create, CreationKind::Create),
        (CreationKind::Create, CreationKind::Fork),
        (CreationKind::Fork, CreationKind::Fork),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("same-{index}");
        let barrier = Arc::new(Barrier::new(2));
        let first_repository = repository.clone();
        let first_cwd = cwd.clone();
        let first_source = source_metadata.clone();
        let first_barrier = barrier.clone();
        let first_id = id.clone();
        let first = thread::spawn(move || {
            first_barrier.wait();
            match first_kind {
                CreationKind::Create => {
                    first_repository.create(create_options(&first_cwd, &first_id))
                }
                CreationKind::Fork => first_repository.fork(
                    &first_source,
                    create_options(&first_cwd, &first_id),
                    ForkOptions::Tree,
                ),
            }
            .map(|session| {
                drop(session);
            })
            .map_err(|error| error.code())
        });
        let second_repository = repository.clone();
        let second_cwd = cwd.clone();
        let second_source = source_metadata.clone();
        let second_id = id.clone();
        let second = thread::spawn(move || {
            barrier.wait();
            match second_kind {
                CreationKind::Create => {
                    second_repository.create(create_options(&second_cwd, &second_id))
                }
                CreationKind::Fork => second_repository.fork(
                    &second_source,
                    create_options(&second_cwd, &second_id),
                    ForkOptions::Tree,
                ),
            }
            .map(|session| {
                drop(session);
            })
            .map_err(|error| error.code())
        });

        let results = [first.join().unwrap(), second.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .copied()
                .collect::<Vec<_>>(),
            vec![SessionErrorCode::AlreadyExists]
        );
        assert_eq!(
            repository
                .list(JsonlSessionListOptions {
                    cwd: Some(cwd.clone()),
                })
                .unwrap()
                .iter()
                .filter(|metadata| metadata.id == id)
                .count(),
            1
        );
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn removes_partial_create_and_releases_destination_claim_after_header_write_failure() {
    let root = temporary_directory("create-write-failure");
    let publisher = Arc::new(FailingSessionFilePublisher::default());
    let repository = JsonlSessionRepository::with_publisher(&root, publisher.clone()).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();

    publisher.fail_once(FAIL_CREATE_WRITE);
    let error = repository
        .create(create_options(&cwd, "retry-create"))
        .unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(
        repository
            .list(JsonlSessionListOptions {
                cwd: Some(cwd.clone()),
            })
            .unwrap()
            .is_empty(),
        "失败的 header 写入不能发布 partial Session"
    );

    let retry = repository
        .create(create_options(&cwd, "retry-create"))
        .unwrap();
    drop(retry);
    assert_eq!(
        repository
            .list(JsonlSessionListOptions { cwd: Some(cwd) })
            .unwrap()
            .iter()
            .filter(|metadata| metadata.id == "retry-create")
            .count(),
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn does_not_delete_a_competing_session_when_create_new_loses_the_race() {
    let root = temporary_directory("create-new-race");
    let publisher = Arc::new(FailingSessionFilePublisher::default());
    let repository = JsonlSessionRepository::with_publisher(&root, publisher.clone()).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();

    publisher.fail_once(FAIL_CREATE_AFTER_COMPETITOR_PUBLISH);
    let error = repository
        .create(create_options(&cwd, "competing-create"))
        .unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::AlreadyExists);

    let session_files = fs::read_dir(&root)
        .unwrap()
        .flat_map(|entry| fs::read_dir(entry.unwrap().path()).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();
    assert_eq!(session_files.len(), 1);
    assert_eq!(
        fs::read_to_string(&session_files[0]).unwrap(),
        "competitor session\n",
        "create-new 失败时不能删除未由本调用创建的目标"
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn does_not_publish_partial_fork_when_staging_append_fails() {
    let root = temporary_directory("fork-staging-failure");
    let publisher = Arc::new(FailingSessionFilePublisher::default());
    let repository = JsonlSessionRepository::with_publisher(&root, publisher.clone()).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();
    let mut source = repository.create(create_options(&cwd, "source")).unwrap();
    source.append_entry(message("root")).unwrap();
    let source_metadata = repository
        .list(JsonlSessionListOptions::default())
        .unwrap()
        .into_iter()
        .find(|metadata| metadata.id == "source")
        .unwrap();
    drop(source);

    publisher.fail_once(FAIL_APPEND);
    let error = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "retry-staging"),
            ForkOptions::Tree,
        )
        .unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(temporary_files(&root).is_empty());
    assert!(
        repository
            .list(JsonlSessionListOptions {
                cwd: Some(cwd.clone()),
            })
            .unwrap()
            .iter()
            .all(|metadata| metadata.id != "retry-staging")
    );

    let retry = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "retry-staging"),
            ForkOptions::Tree,
        )
        .unwrap();
    drop(retry);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn does_not_publish_fork_and_releases_claim_when_atomic_rename_fails() {
    let root = temporary_directory("fork-rename-failure");
    let publisher = Arc::new(FailingSessionFilePublisher::default());
    let repository = JsonlSessionRepository::with_publisher(&root, publisher.clone()).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();
    let mut source = repository.create(create_options(&cwd, "source")).unwrap();
    source.append_entry(message("root")).unwrap();
    let source_metadata = repository
        .list(JsonlSessionListOptions::default())
        .unwrap()
        .into_iter()
        .find(|metadata| metadata.id == "source")
        .unwrap();
    drop(source);

    publisher.fail_once(FAIL_RENAME);
    let error = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "retry-rename"),
            ForkOptions::Tree,
        )
        .unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert!(temporary_files(&root).is_empty());
    assert!(
        repository
            .list(JsonlSessionListOptions {
                cwd: Some(cwd.clone()),
            })
            .unwrap()
            .iter()
            .all(|metadata| metadata.id != "retry-rename")
    );

    let retry = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "retry-rename"),
            ForkOptions::Tree,
        )
        .unwrap();
    drop(retry);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn releases_fork_destination_claim_after_a_validation_failure() {
    let root = temporary_directory("fork-claim-release");
    let repository = JsonlSessionRepository::new(&root).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();
    let mut source = repository.create(create_options(&cwd, "source")).unwrap();
    source.append_entry(message("root")).unwrap();
    let source_metadata = repository
        .list(JsonlSessionListOptions::default())
        .unwrap()
        .into_iter()
        .find(|metadata| metadata.id == "source")
        .unwrap();
    drop(source);

    // fork 在取得目标 claim 后才解析复制范围。失败必须释放 sidecar claim，
    // 否则同一 `{cwd, id}` 会被永久占用，和 TypeScript reservation 的 finally
    // 释放语义不一致。
    let first_attempt = repository.fork(
        &source_metadata,
        create_options(&cwd, "retry"),
        ForkOptions::Branch {
            entry_id: Some("missing".into()),
            position: None,
        },
    );
    assert_eq!(
        first_attempt.unwrap_err().code(),
        SessionErrorCode::InvalidForkTarget
    );
    let retry = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "retry"),
            ForkOptions::Tree,
        )
        .unwrap();
    drop(retry);
    assert_eq!(
        repository
            .list(JsonlSessionListOptions { cwd: Some(cwd) })
            .unwrap()
            .iter()
            .filter(|metadata| metadata.id == "retry")
            .count(),
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn fork_default_and_explicit_target_positions_match_typescript_semantics() {
    let root = temporary_directory("fork-targets");
    let repository = JsonlSessionRepository::new(&root).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();
    let mut source = repository.create(create_options(&cwd, "source")).unwrap();
    source.append_entry(message("root")).unwrap();
    source.append_entry(message("tail")).unwrap();
    let source_metadata = repository
        .list(JsonlSessionListOptions::default())
        .unwrap()
        .remove(0);
    drop(source);

    // TypeScript 的规则较容易混淆：显式 entryId 默认 `before`，未显式
    // 指定 entryId 时则默认选 main leaf 并使用 `at`。两者必须独立回归，
    // 否则 fork 会悄然多复制或少复制一个上下文 entry。
    let explicit_before = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "explicit-before"),
            ForkOptions::Branch {
                entry_id: Some("tail".into()),
                position: None,
            },
        )
        .unwrap();
    assert_eq!(
        explicit_before
            .state()
            .find_entries(EntryQuery {
                oldest_first: true,
                ..Default::default()
            })
            .unwrap()
            .iter()
            .map(|entry| entry.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec![Some("root")]
    );
    drop(explicit_before);

    let default_at = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "default-at"),
            ForkOptions::Branch {
                entry_id: None,
                position: None,
            },
        )
        .unwrap();
    assert_eq!(default_at.state().lane_leaf("main"), Some(Some("tail")));
    drop(default_at);

    let invalid_target = repository.fork(
        &source_metadata,
        create_options(&cwd, "missing-target"),
        ForkOptions::Branch {
            entry_id: Some("missing".into()),
            position: None,
        },
    );
    assert_eq!(
        invalid_target.unwrap_err().code(),
        SessionErrorCode::InvalidForkTarget
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn classifies_repository_lane_and_default_fork_target_failures_like_typescript() {
    let root = temporary_directory("error-codes");
    let repository = JsonlSessionRepository::new(&root).unwrap();
    let cwd = root.join("project").to_string_lossy().into_owned();
    let mut source = repository.create(create_options(&cwd, "source")).unwrap();

    // TypeScript 区分重复 lane、未知 lane 与不可 fork 的 custom leaf；Rust 通过
    // `code()` 暴露同一稳定分类，调用方无需解析诊断文本。
    assert_eq!(
        source.create_lane("main", None).unwrap_err().code(),
        SessionErrorCode::AlreadyExists
    );
    assert_eq!(
        source.move_lane("missing", None).unwrap_err().code(),
        SessionErrorCode::InvalidLane
    );
    source
        .append_entry(NewEntry {
            lane: "main".into(),
            fields: Map::from_iter([
                (String::from("id"), Value::String("custom-leaf".into())),
                (String::from("type"), Value::String("custom".into())),
                (String::from("customType"), Value::String("note".into())),
            ]),
        })
        .unwrap();
    let source_metadata = repository
        .list(JsonlSessionListOptions::default())
        .unwrap()
        .remove(0);
    drop(source);

    let error = repository
        .fork(
            &source_metadata,
            create_options(&cwd, "custom-leaf-fork"),
            ForkOptions::Branch {
                entry_id: None,
                position: None,
            },
        )
        .unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::InvalidForkTarget);
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
