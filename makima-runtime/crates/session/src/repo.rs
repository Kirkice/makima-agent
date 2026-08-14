//! JSONL v4 Session 仓库。
//!
//! 本模块只编排文件系统边界：目录布局、header 元数据发现、创建竞争保护、
//! fork 的临时文件发布和跨进程单写者锁。JSONL mutation 的编解码、状态归约
//! 与 fork 内容投影仍由上层 [`JsonlSessionStore`] 负责，避免 repository 持有
//! 两份业务规则。

use crate::{
    ForkPosition, JsonlSessionStore, SessionFilePublisher, SessionHeader, SessionStoreError,
    StandardSessionFilePublisher,
};
use fs2::FileExt;
use serde_json::{Map, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static GENERATED_IDS: AtomicU64 = AtomicU64::new(0);

/// 从 JSONL v4 header 与文件系统属性派生出的 Session 元数据快照。
///
/// `path` 是实际数据文件，`modified_at` 来自文件系统的最后修改时间；两者与
/// TypeScript repository 的列表返回值保持相同含义。
#[derive(Debug, Clone, PartialEq)]
pub struct JsonlSessionMetadata {
    pub id: String,
    pub created_at: u64,
    pub cwd: String,
    pub path: PathBuf,
    pub modified_at: u64,
    pub parent_session_id: Option<String>,
    pub legacy_parent_session_path: Option<String>,
    pub metadata: Option<Map<String, Value>>,
    pub source_format: u32,
}

/// 创建或 fork 目标 Session 时写入 header 的输入。
///
/// `cwd` 会先解析为绝对路径，再映射到与 TypeScript 相同的目录名。未提供
/// `id` 时由 repository 生成合法且进程内唯一的 ID。
#[derive(Debug, Clone, PartialEq)]
pub struct JsonlSessionCreateOptions {
    pub cwd: String,
    pub id: Option<String>,
    pub parent_session_id: Option<String>,
    pub metadata: Option<Map<String, Value>>,
}

/// 列表查询条件；省略 `cwd` 时扫描所有 Session 工作目录。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsonlSessionListOptions {
    pub cwd: Option<String>,
}

/// 与 TypeScript JSONL 后端一致的 fork 范围。
///
/// `Branch` 仅复制目标 message 的 root-to-target 链并重建 `main` lane；`Tree`
/// 复制全部 entry 以及所有 lane。省略 `position` 时保留 TypeScript 的默认值：
/// 显式 entry 使用 `Before`，隐式当前 main 叶节点使用 `At`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkOptions {
    Branch {
        entry_id: Option<String>,
        position: Option<ForkPosition>,
    },
    Tree,
}

/// 已打开的 Store 及其跨进程独占写者租约。
///
/// 租约锁定数据文件的 sidecar 文件，而不锁 JSONL 数据文件本身。这样既规避
/// Windows 上数据文件锁与打开模式的差异，也允许 fork 通过 rename 原子发布。
/// 句柄析构时操作系统释放锁；调用方必须持有此值才能可变访问 Store。
#[derive(Debug)]
pub struct LeasedJsonlSession {
    store: JsonlSessionStore,
    _writer_lease: File,
}

impl Deref for LeasedJsonlSession {
    type Target = JsonlSessionStore;

    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl DerefMut for LeasedJsonlSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.store
    }
}

impl LeasedJsonlSession {
    /// 取回 Store 并释放跨进程单写者租约。
    ///
    /// 此操作会使调用方失去 repository 层面的排他写入保证；生产 runtime 通常应
    /// 直接丢弃 lease，而不是调用该方法。
    pub fn into_store(self) -> JsonlSessionStore {
        self.store
    }
}

/// 与 TypeScript JSONL v4 目录布局兼容的文件系统 repository。
///
/// 每个绝对 `cwd` 映射到根目录下的一个子目录。数据文件使用 ISO UTC 时间戳
/// 和 Session ID 命名；列表和打开以 header 为权威来源，不依赖文件名反解析。
#[derive(Debug, Clone)]
pub struct JsonlSessionRepository {
    sessions_root: PathBuf,
    publisher: std::sync::Arc<dyn SessionFilePublisher>,
}

impl JsonlSessionRepository {
    /// 创建 repository，并将根路径解析为绝对路径。
    ///
    /// 该操作不创建目录，保持与 TypeScript repository 构造函数相同的惰性行为。
    pub fn new(sessions_root: impl Into<PathBuf>) -> Result<Self, SessionStoreError> {
        Self::with_publisher(
            sessions_root,
            std::sync::Arc::new(StandardSessionFilePublisher),
        )
    }

    /// 使用可替换发布器构造 repository。
    ///
    /// 该入口只替换持久化写入原语，不改变目录布局、锁、状态归约或查询逻辑；
    /// 因此故障注入测试可以覆盖真实编排，而无需维护一套测试专用 repository。
    pub fn with_publisher(
        sessions_root: impl Into<PathBuf>,
        publisher: std::sync::Arc<dyn SessionFilePublisher>,
    ) -> Result<Self, SessionStoreError> {
        let sessions_root = absolute_path(sessions_root.into())?;
        Ok(Self {
            sessions_root,
            publisher,
        })
    }

    /// 创建空 Session 并返回已持有独占写者租约的 Store。
    ///
    /// 先取得目标 sidecar 写锁，再创建数据文件，因此文件一旦对其他进程可见，
    /// 调用方已经是唯一 writer。独立的 create lock 仅负责防止两个创建者为同一
    /// `{cwd, id}` 同时通过存在性检查。
    pub fn create(
        &self,
        options: JsonlSessionCreateOptions,
    ) -> Result<LeasedJsonlSession, SessionStoreError> {
        let (id, cwd, directory) = self.resolve_destination(&options)?;
        fs::create_dir_all(&directory)?;
        let create_claim = acquire_lock(create_lock_path(&directory, &id))?;
        if self.session_id_exists(&directory, &id)? {
            return Err(SessionStoreError::InvalidArgument(format!(
                "session already exists: {id}"
            )));
        }
        let created_at = unix_millis();
        let path = directory.join(session_file_name(created_at, &id));
        let writer_lease = acquire_lock(writer_lock_path(&path))?;
        let header = SessionHeader {
            kind: "header".into(),
            version: 4,
            id,
            created_at,
            cwd,
            parent_session_id: options.parent_session_id,
            legacy_parent_session_path: None,
            metadata: options.metadata,
        };
        let store = JsonlSessionStore::create_with_header_and_publisher(
            &path,
            header,
            self.publisher.clone(),
        )?;
        drop(create_claim);
        Ok(LeasedJsonlSession {
            store,
            _writer_lease: writer_lease,
        })
    }

    /// 以元数据指定的数据文件打开 Session，并取得独占写者租约。
    ///
    /// 此 API 是写入入口；只读发现应使用 [`Self::list`]，后者不会竞争写锁。
    pub fn open(
        &self,
        metadata: &JsonlSessionMetadata,
    ) -> Result<LeasedJsonlSession, SessionStoreError> {
        if !metadata.path.exists() {
            return Err(SessionStoreError::InvalidArgument(format!(
                "session not found: {}",
                metadata.id
            )));
        }
        let writer_lease = acquire_lock(writer_lock_path(&metadata.path))?;
        let store = JsonlSessionStore::open_with_publisher(&metadata.path, self.publisher.clone())?;
        if store.header().id != metadata.id {
            return Err(SessionStoreError::InvalidArgument(format!(
                "session id does not match header: {}",
                metadata.id
            )));
        }
        Ok(LeasedJsonlSession {
            store,
            _writer_lease: writer_lease,
        })
    }

    /// 读取符合 v4 header 的 Session 元数据，按修改时间降序返回。
    ///
    /// 列表绝不获取 writer lease，因此状态界面可以读取活动 Session；损坏的、
    /// 空的或非 v4 JSONL 文件会被忽略，与 TypeScript `listDirect()` 一致。
    pub fn list(
        &self,
        options: JsonlSessionListOptions,
    ) -> Result<Vec<JsonlSessionMetadata>, SessionStoreError> {
        let directories = match options.cwd {
            Some(cwd) => {
                let cwd = absolute_cwd(&cwd)?;
                let directory = self.sessions_root.join(session_directory_name(&cwd));
                if directory.exists() {
                    vec![directory]
                } else {
                    Vec::new()
                }
            }
            None => {
                if !self.sessions_root.exists() {
                    Vec::new()
                } else {
                    fs::read_dir(&self.sessions_root)?
                        .filter_map(Result::ok)
                        .filter(|entry| {
                            entry
                                .file_type()
                                .is_ok_and(|kind| kind.is_dir() || kind.is_symlink())
                        })
                        .map(|entry| entry.path())
                        .collect()
                }
            }
        };
        let mut result = Vec::new();
        for directory in directories {
            for entry in fs::read_dir(directory)?.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Some(metadata) = read_metadata(&path)? {
                    result.push(metadata);
                }
            }
        }
        result.sort_by(|left, right| right.modified_at.cmp(&left.modified_at));
        Ok(result)
    }

    /// 删除数据文件；不存在的文件按 TypeScript `force: true` 语义视为成功。
    ///
    /// 删除前会短暂取得 writer lease，避免删除与同一文件的写入并发执行。sidecar
    /// 锁文件会保留：文件名只是锁的稳定定位符，删除它会使新创建的锁文件与仍在
    /// 持有的旧文件锁分裂，从而破坏跨进程互斥。
    pub fn delete(&self, metadata: &JsonlSessionMetadata) -> Result<(), SessionStoreError> {
        let _writer_lease = acquire_lock(writer_lock_path(&metadata.path))?;
        if metadata.path.exists() {
            fs::remove_file(&metadata.path)?;
        }
        Ok(())
    }

    /// 从 source 创建 branch 或 tree fork，并以原子 rename 发布目标文件。
    ///
    /// source 在复制期间保持 writer lease，目标则在临时文件写入前取得最终路径的
    /// writer lease。故 rename 成功后任何竞争打开者都会被阻塞，fork 调用方也不
    /// 会出现“已发布但未拿到写锁”的窗口。
    pub fn fork(
        &self,
        source: &JsonlSessionMetadata,
        mut create: JsonlSessionCreateOptions,
        options: ForkOptions,
    ) -> Result<LeasedJsonlSession, SessionStoreError> {
        let source_store = self.open(source)?;
        if create.parent_session_id.is_none() {
            create.parent_session_id = Some(source.id.clone());
        }
        let (id, cwd, directory) = self.resolve_destination(&create)?;
        fs::create_dir_all(&directory)?;
        let create_claim = acquire_lock(create_lock_path(&directory, &id))?;
        if self.session_id_exists(&directory, &id)? {
            return Err(SessionStoreError::InvalidArgument(format!(
                "session already exists: {id}"
            )));
        }
        let created_at = unix_millis();
        let path = directory.join(session_file_name(created_at, &id));
        let temporary = directory.join(format!(
            ".{}.{}.tmp",
            id,
            GENERATED_IDS.fetch_add(1, Ordering::Relaxed)
        ));
        let header = SessionHeader {
            kind: "header".into(),
            version: 4,
            id,
            created_at,
            cwd,
            parent_session_id: create.parent_session_id,
            legacy_parent_session_path: None,
            metadata: create.metadata,
        };
        let result = (|| {
            let writer_lease = acquire_lock(writer_lock_path(&path))?;
            let mut target = JsonlSessionStore::create_with_header_and_publisher(
                &temporary,
                header,
                self.publisher.clone(),
            )?;
            for mutation in source_store.fork_mutations(options)? {
                target.append(mutation.kind, mutation.payload)?;
            }
            drop(target);
            self.publisher.rename(&temporary, &path)?;
            let store = JsonlSessionStore::open_with_publisher(&path, self.publisher.clone())?;
            Ok(LeasedJsonlSession {
                store,
                _writer_lease: writer_lease,
            })
        })();
        if result.is_err() {
            let _ = self.publisher.remove_file(&temporary);
        }
        drop(create_claim);
        result
    }

    /// 将调用方输入转换为唯一的逻辑目标。
    ///
    /// 此处只解析 ID、绝对工作目录和目录布局，不触碰文件系统；因此 create 与
    /// fork 共享完全一致的路径规则，避免两条创建路径逐渐偏离 TypeScript 格式。
    fn resolve_destination(
        &self,
        options: &JsonlSessionCreateOptions,
    ) -> Result<(String, String, PathBuf), SessionStoreError> {
        let id = options.id.clone().unwrap_or_else(generated_id);
        validate_session_id(&id)?;
        let cwd = absolute_cwd(&options.cwd)?;
        let directory = self.sessions_root.join(session_directory_name(&cwd));
        Ok((id, cwd, directory))
    }

    /// 依据 TypeScript 使用的 `_{id}.jsonl` 后缀检测逻辑 ID 是否已经发布。
    ///
    /// 文件名时间戳不是身份的一部分；同一个工作目录中的 ID 必须全局唯一。
    fn session_id_exists(&self, directory: &Path, id: &str) -> Result<bool, SessionStoreError> {
        if !directory.exists() {
            return Ok(false);
        }
        let suffix = format!("_{id}.jsonl");
        Ok(fs::read_dir(directory)?
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().ends_with(&suffix)))
    }
}

/// 仅读取第一行 header 生成元数据，避免列表操作加载完整 Session 日志。
///
/// TypeScript `listDirect()` 会跳过不可解析的 header；这里保持相同策略。文件
/// 系统读取错误仍向上传播，因为这不是单个 Session 格式不兼容，而是 repository
/// 根目录无法可靠读取。
fn read_metadata(path: &Path) -> Result<Option<JsonlSessionMetadata>, SessionStoreError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let Ok(header) = serde_json::from_str::<SessionHeader>(line.trim_end()) else {
        return Ok(None);
    };
    if header.kind != "header" || header.version != 4 {
        return Ok(None);
    }
    let modified_at = fs::metadata(path)?
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |value| value.as_millis() as u64);
    Ok(Some(JsonlSessionMetadata {
        id: header.id,
        created_at: header.created_at,
        cwd: header.cwd,
        path: path.to_owned(),
        modified_at,
        parent_session_id: header.parent_session_id,
        legacy_parent_session_path: header.legacy_parent_session_path,
        metadata: header.metadata,
        source_format: header.version,
    }))
}

/// 非阻塞地取得 sidecar 文件的独占锁。
///
/// 返回的 [`File`] 本身就是租约：只要其存活，其他进程的 `try_lock_exclusive`
/// 就会失败；进程异常退出时由操作系统释放。锁文件保留在磁盘上不代表仍被占用。
fn acquire_lock(path: PathBuf) -> Result<File, SessionStoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;
    file.try_lock_exclusive().map_err(|error| {
        SessionStoreError::InvalidArgument(format!("session writer is already active: {error}"))
    })?;
    Ok(file)
}

/// 按 TypeScript filesystem `absolutePath()` 的基本语义解析相对路径。
///
/// 不调用 canonicalize：目标目录或 Session 文件在创建前可能尚不存在，且
/// TypeScript 后端也不会要求路径已存在才能创建。
fn absolute_path(path: PathBuf) -> Result<PathBuf, SessionStoreError> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

/// 校验工作目录非空，并统一转换为目录布局所用的绝对字符串。
fn absolute_cwd(cwd: &str) -> Result<String, SessionStoreError> {
    if cwd.is_empty() {
        return Err(SessionStoreError::InvalidArgument(
            "session cwd cannot be empty".into(),
        ));
    }
    Ok(absolute_path(PathBuf::from(cwd))?
        .to_string_lossy()
        .into_owned())
}

/// 校验 TypeScript `SESSION_ID_PATTERN` 对应的 ASCII Session ID 规则。
fn validate_session_id(id: &str) -> Result<(), SessionStoreError> {
    let bytes = id.as_bytes();
    if bytes.is_empty()
        || !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(SessionStoreError::InvalidArgument(
            "session id must contain only alphanumeric characters, '-', '_' and '.', and start/end with an alphanumeric character".into(),
        ));
    }
    Ok(())
}

/// 将绝对工作目录映射成 TypeScript JSONL repository 使用的目录名。
fn session_directory_name(cwd: &str) -> String {
    let stripped = cwd.trim_start_matches(['/', '\\']);
    format!("--{}--", stripped.replace(['/', '\\', ':'], "-"))
}

/// 生成 TypeScript `sessionFileName()` 等价的 ISO UTC 数据文件名。
fn session_file_name(created_at: u64, id: &str) -> String {
    format!("{}_{}.jsonl", iso_utc_timestamp(created_at), id)
}

/// 将 Unix 毫秒时间戳格式化为 TypeScript `Date#toISOString()` 替换 `:`、`.` 后的形式。
///
/// crate 不引入时间库，仅为文件名布局实现无时区、无本地化依赖的 UTC 转换。
fn iso_utc_timestamp(created_at: u64) -> String {
    let total_seconds = created_at / 1_000;
    let milliseconds = created_at % 1_000;
    let days = total_seconds / 86_400;
    let seconds_of_day = total_seconds % 86_400;
    let (year, month, day) = civil_date_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}-{milliseconds:03}Z")
}

/// 将自 1970-01-01 起的整日数转换为公历日期。
///
/// 算法采用 Howard Hinnant 的 civil-from-days 公式；输入来自 Unix 毫秒时间戳，
/// 因此不会涉及本地时区。超出 `i64` 的理论输入饱和到最大日期，仅用于文件名，
/// 正常系统时间范围内与 JavaScript UTC ISO 格式完全一致。
fn civil_date_from_days(days_since_epoch: u64) -> (i64, u32, u32) {
    let days = i64::try_from(days_since_epoch)
        .unwrap_or(i64::MAX)
        .saturating_add(719_468);
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = month_index + if month_index < 10 { 3 } else { -9 };
    (
        year + if month <= 2 { 1 } else { 0 },
        month as u32,
        day as u32,
    )
}

/// 返回数据文件对应的 writer sidecar 路径。
fn writer_lock_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.writer.lock", path.to_string_lossy()))
}

/// 返回按逻辑 `{cwd, id}` 互斥的创建锁路径。
fn create_lock_path(directory: &Path, id: &str) -> PathBuf {
    directory.join(format!(".{id}.create.lock"))
}

/// 生成符合 Session ID 规则的进程内唯一默认 ID。
///
/// TypeScript 使用 UUIDv7；这里采用毫秒时间戳和单调计数器以保持合法格式及排序
/// 友好性。跨进程冲突仍由创建锁和已发布文件检查处理。
fn generated_id() -> String {
    format!(
        "rust-{}-{}",
        unix_millis(),
        GENERATED_IDS.fetch_add(1, Ordering::Relaxed)
    )
}

/// 返回 Unix epoch 毫秒；系统时钟异常早于 epoch 时回退为零。
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::{iso_utc_timestamp, session_file_name};

    #[test]
    fn session_file_name_matches_typescript_iso_utc_layout() {
        assert_eq!(
            session_file_name(1_700_000_000_123, "session-1"),
            "2023-11-14T22-13-20-123Z_session-1.jsonl"
        );
        assert_eq!(iso_utc_timestamp(0), "1970-01-01T00-00-00-000Z");
    }
}
