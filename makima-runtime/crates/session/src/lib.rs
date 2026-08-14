//! Pi Agent 的 Session Store 持久化边界。
//!
//! 本 crate 只负责“有序 JSONL 日志”的文件读写和基础完整性校验，不负责
//! Agent Loop、Provider、工具执行或 TUI。这样 Session Store 可以独立测试，
//! 上层运行时也可以在迁移期间继续使用 TypeScript 实现。

// 创建与打开 Session 文件

// 创建 v4 JSONL header：Session ID、创建时间、工作目录、父 Session、元数据，见 SessionHeader。
// 从磁盘恢复完整内存状态，见 JsonlSessionStore::open()。
// 可靠追加与恢复

// 全局递增 sequence，保证日志顺序连续，见 JsonlSessionStore::append()。
// 先用状态副本校验 mutation，再写磁盘；只有落盘并同步成功才提交内存状态，防止内存和文件分叉，见 JsonlSessionStore::append()。
// 进程中断导致最后一行 JSON 未写完整时，恢复时安全截断该尾行；中间损坏则报错，见 JsonlSessionStore::open()。
// 保存对话树与分支

// 写入 entry，例如用户消息、助手消息、模型切换、压缩记录或扩展自定义事件，见 JsonlSessionStore::append_entry()。
// 自动补齐 parentId、seq、timestamp，由 Store 控制父子链和顺序，避免上层写出断裂分支。
// 管理 lane（分支指针）：创建分支、移动分支到指定 entry，见 JsonlSessionStore::create_lane() 与 JsonlSessionStore::move_lane()。
// 保存运行过程记录

// 写入 operation started/finished、取消、工具执行、队列变化、usage 等 record，见 JsonlSessionStore::append_record()。
// 同一 lane 不允许并行启动多个 operation，确保一次 Agent Run 的日志边界清晰，见 JsonlSessionStore::append_record()。
// 维护 Session 元信息

// Session 名称，见 JsonlSessionStore::set_name()。
// entry 标签，见 JsonlSessionStore::set_label()。

mod repo;
mod state;

pub use repo::{
    ForkOptions, JsonlSessionCreateOptions, JsonlSessionListOptions, JsonlSessionMetadata,
    JsonlSessionRepository, LeasedJsonlSession,
};
pub use state::{
    BranchBounds, EntryQuery, LanePointer, LogOptions, RecordQuery, SessionEntry, SessionRecord,
    SessionState, SessionStats,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FORMAT_VERSION: u32 = 4;

/// JSONL 文件的第一行。字段名与现有 TypeScript v4 存储保持一致。
///
/// Rust 内部继续使用 snake_case，以符合 Rust 的命名习惯；`serde` 在文件
/// 边界将它转换为 camelCase。因此 Rust 新建的文件可被 TypeScript v4 Store
/// 直接读取，反向读取 TypeScript 创建的文件也不会丢失父 Session 信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHeader {
    pub kind: String,
    pub version: u32,
    pub id: String,
    pub created_at: u64,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_parent_session_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Map<String, Value>>,
}

/// 一条可持久化的 Session 变更。
///
/// Rust 第一阶段不复制 TypeScript 的全部强类型 Entry/Record 联合类型，
/// 而是保留 JSON payload。这样既能兼容已有字段，也避免在协议尚未冻结时
/// 把 Provider 消息模型错误地固化到 Rust Core。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMutation {
    pub kind: String,
    pub seq: u64,
    #[serde(flatten)]
    pub payload: serde_json::Map<String, Value>,
}

/// 由 Store 补齐 parent、sequence 与时间戳之前的 entry 输入。
///
/// `fields` 必须包含 v4 entry 的 `id` 与 `type`，其余 Provider/Extension 私有字段
/// 原样保留。调用方不能覆盖 Store 所有的 `parentId`、`seq` 和 `timestamp`。
#[derive(Debug, Clone, PartialEq)]
pub struct NewEntry {
    pub lane: String,
    pub fields: serde_json::Map<String, Value>,
}

/// 由 Store 补齐 sequence 与时间戳之前的 record 输入。
///
/// `fields` 必须包含 v4 record 的 `id`、`type` 和 `lane`。lane 同时单独保留，
/// 以便调用方在构造期明确选择写入目标并防止 fields 与参数不一致。
#[derive(Debug, Clone, PartialEq)]
pub struct NewRecord {
    pub lane: String,
    pub fields: serde_json::Map<String, Value>,
}

/// 与 TypeScript `SessionErrorCode` 一一对应的稳定错误分类。
///
/// Rust 保留底层 I/O 和格式错误的详细信息，但上层 RPC 或跨语言 conformance
/// 不应依赖英文错误文本。通过 [`SessionStoreError::code`] 可以将不同后端的失败
/// 归类为同一组可恢复、可展示的语义错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionErrorCode {
    NotFound,
    AlreadyExists,
    InvalidEntry,
    InvalidPayload,
    InvalidLane,
    InvalidQuery,
    InvalidForkTarget,
    Storage,
}

/// Session Store 的统一错误。
#[derive(Debug)]
pub enum SessionStoreError {
    Io(io::Error),
    InvalidFormat { line: usize, message: String },
    InvalidArgument(String),
    InvalidMutation(String),
    SequenceGap { expected: u64, actual: u64 },
}

impl SessionStoreError {
    /// 将 Rust 细粒度错误投影为 TypeScript Session API 的稳定分类。
    ///
    /// 当前错误枚举仍需保留诊断文本和 I/O 原因；该方法只为 API 边界和
    /// conformance 测试提供稳定分类，不允许调用方再从错误文本推导语义。
    pub fn code(&self) -> SessionErrorCode {
        match self {
            // `create_new` 是最终的跨进程竞争裁决点。即使前置 exists 检查未命中，
            // 此处的 AlreadyExists 仍属于稳定的目标冲突，而不是一般存储故障。
            Self::Io(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                SessionErrorCode::AlreadyExists
            }
            Self::Io(_) | Self::InvalidFormat { .. } => SessionErrorCode::Storage,
            Self::SequenceGap { .. } => SessionErrorCode::InvalidEntry,
            Self::InvalidArgument(message) | Self::InvalidMutation(message) => {
                classify_session_error(message)
            }
        }
    }
}

/// 将尚未迁移为带分类字段的历史错误文本集中映射为公开错误码。
///
/// 这是过渡边界：状态机与 repository 内部仍可保留包含上下文的诊断文本，而
/// 所有外部调用方只读取 [`SessionStoreError::code`]。新增失败路径必须复用这里
/// 的稳定词汇，避免把文本匹配扩散到 RPC 或测试代码。
fn classify_session_error(message: &str) -> SessionErrorCode {
    if message.contains("fork target") {
        SessionErrorCode::InvalidForkTarget
    } else if message.contains("already has an open operation") {
        // TypeScript 将持久化层的单 operation 约束报告为 storage；保持该行为，
        // 以便调用方把该失败视为 writer/recovery 边界，而非普通输入错误。
        SessionErrorCode::Storage
    } else if message.contains("lane not found") || message.contains("missing lane") {
        SessionErrorCode::InvalidLane
    } else if message.contains("entry not found")
        || message.contains("session not found")
        || message.contains("missing target")
    {
        SessionErrorCode::NotFound
    } else if message.contains("duplicate")
        || message.contains("already exists")
        || message.contains("already active")
    {
        SessionErrorCode::AlreadyExists
    } else if message.contains("limit")
        || message.contains("cursor")
        || message.contains("operation kind")
    {
        SessionErrorCode::InvalidQuery
    } else if message.contains("parent")
        || message.contains("cycle")
        || message.contains("unsupported entry")
        || message.contains("unsupported record")
        || message.contains("sequence")
    {
        SessionErrorCode::InvalidEntry
    } else {
        SessionErrorCode::InvalidPayload
    }
}

impl std::fmt::Display for SessionStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "session storage I/O error: {error}"),
            Self::InvalidFormat { line, message } => {
                write!(
                    formatter,
                    "invalid session format at line {line}: {message}"
                )
            }
            Self::InvalidArgument(message) => {
                write!(formatter, "invalid session argument: {message}")
            }
            Self::InvalidMutation(message) => {
                write!(formatter, "invalid session mutation: {message}")
            }
            Self::SequenceGap { expected, actual } => {
                write!(
                    formatter,
                    "session sequence gap: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for SessionStoreError {}

impl From<io::Error> for SessionStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Session JSONL 文件的最小可变边界。
///
/// 读取与目录发现仍直接使用标准库；只有会改变持久化状态的四类操作经由该端口。
/// 这样 repository 与 store 可以在测试中精确注入 create、append、rename 失败，
/// 同时不会把完整文件系统抽象泄漏到状态归约和查询模块。
pub trait SessionFilePublisher: std::fmt::Debug + Send + Sync {
    fn create_new(&self, path: &Path) -> io::Result<File>;
    fn append(&self, path: &Path) -> io::Result<File>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
}

/// 生产环境使用的标准文件系统发布器。
#[derive(Debug, Default)]
pub struct StandardSessionFilePublisher;

impl SessionFilePublisher for StandardSessionFilePublisher {
    fn create_new(&self, path: &Path) -> io::Result<File> {
        OpenOptions::new().write(true).create_new(true).open(path)
    }

    fn append(&self, path: &Path) -> io::Result<File> {
        OpenOptions::new().append(true).open(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

/// 一个追加式 JSONL Session Store。
///
/// 每次追加都先写入完整的一行，再更新内存中的 sequence。若写入失败，
/// 内存状态不会前进，下一次写入仍然从同一个 sequence 开始，避免产生
/// “内存认为成功、磁盘实际没有”的状态分叉。
#[derive(Debug)]
pub struct JsonlSessionStore {
    path: PathBuf,
    header: SessionHeader,
    mutations: Vec<SessionMutation>,
    state: SessionState,
    next_sequence: u64,
    publisher: std::sync::Arc<dyn SessionFilePublisher>,
}

impl JsonlSessionStore {
    /// 创建一个新文件并写入 v4 header；目标文件已存在时拒绝覆盖。
    pub fn create(
        path: impl Into<PathBuf>,
        id: impl Into<String>,
        cwd: impl Into<String>,
    ) -> Result<Self, SessionStoreError> {
        Self::create_with_header_and_publisher(
            path,
            SessionHeader {
                kind: "header".into(),
                version: FORMAT_VERSION,
                id: id.into(),
                created_at: unix_millis(),
                cwd: cwd.into(),
                parent_session_id: None,
                legacy_parent_session_path: None,
                metadata: None,
            },
            std::sync::Arc::new(StandardSessionFilePublisher),
        )
    }

    /// 使用完整 v4 header 与指定发布器创建 Store，供 repository 和故障注入测试使用。
    pub(crate) fn create_with_header_and_publisher(
        path: impl Into<PathBuf>,
        header: SessionHeader,
        publisher: std::sync::Arc<dyn SessionFilePublisher>,
    ) -> Result<Self, SessionStoreError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(SessionStoreError::InvalidArgument(
                "session path cannot be empty".into(),
            ));
        }
        validate_header(&header)?;
        if path.exists() {
            return Err(SessionStoreError::InvalidArgument(format!(
                "session already exists: {}",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // 只有本次调用成功取得 create-new 句柄后，才拥有失败时删除目标的权限。
        // 若 create-new 本身失败，文件可能由并发创建者在 exists 检查后发布；此时
        // 删除路径会破坏对方已成功创建的 Session。取得句柄后的 header 写入或
        // sync 失败则必须清理 partial 文件，且清理错误不能覆盖原始写入错误。
        let mut file = publisher.create_new(&path)?;
        let create_result = (|| -> Result<(), SessionStoreError> {
            write_json_line(&mut file, &header)?;
            file.sync_all()?;
            Ok(())
        })();
        if let Err(error) = create_result {
            drop(file);
            let _ = publisher.remove_file(&path);
            return Err(error);
        }

        Ok(Self {
            path,
            header,
            mutations: Vec::new(),
            state: SessionState::new(),
            next_sequence: 1,
            publisher,
        })
    }

    /// 从已有 JSONL 文件恢复内存索引。
    ///
    /// 最后一行如果是未完成的 JSON（典型的进程中断写入），只截断到最后
    /// 一个完整换行符；中间位置的损坏则直接失败，避免悄悄丢失历史数据。
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, SessionStoreError> {
        Self::open_with_publisher(path, std::sync::Arc::new(StandardSessionFilePublisher))
    }

    pub(crate) fn open_with_publisher(
        path: impl Into<PathBuf>,
        publisher: std::sync::Arc<dyn SessionFilePublisher>,
    ) -> Result<Self, SessionStoreError> {
        let path = path.into();
        let content = fs::read_to_string(&path)?;
        let physical_lines: Vec<&str> = content.split_inclusive('\n').collect();
        let header_line = physical_lines
            .first()
            .ok_or_else(|| invalid_format(1, "missing session header"))?
            .trim_end_matches(['\r', '\n']);
        if header_line.is_empty() {
            return Err(invalid_format(1, "missing session header"));
        }
        let header: SessionHeader = parse_line(header_line, 1)?;
        if let Err(error) = validate_header(&header) {
            return Err(invalid_format(1, error.to_string()));
        }

        let mut mutations = Vec::new();
        let mut state = SessionState::new();
        let mut byte_offset = physical_lines[0].len();
        let mut repaired_torn_tail = false;
        for (index, physical_line) in physical_lines.iter().enumerate().skip(1) {
            let line_number = index + 1;
            let line = physical_line.trim_end_matches(['\r', '\n']);
            let is_last_physical_line = index + 1 == physical_lines.len();
            match serde_json::from_str::<SessionMutation>(line) {
                Ok(mutation) => {
                    if mutation.kind.is_empty() {
                        return Err(invalid_format(line_number, "mutation kind is empty"));
                    }
                    if let Err(error) = state.apply(&mutation) {
                        return match error {
                            SessionStoreError::SequenceGap { .. } => Err(error),
                            other => Err(invalid_format(line_number, other.to_string())),
                        };
                    }
                    mutations.push(mutation);
                    byte_offset += physical_line.len();
                }
                // 只有最后一个物理行的 JSON 语法错误才能视为进程中断造成的
                // torn tail。数据结构错误和中间行错误都必须返回给调用方。
                Err(error) if is_last_physical_line && (error.is_syntax() || error.is_eof()) => {
                    // 不直接截断原文件。TypeScript Store 会先写出完整有效前缀，
                    // 再以 rename 原子替换；这样在修复过程中断或存储失败时，调用
                    // 方仍能保留原始损坏文件用于再次恢复或人工排查。
                    repair_torn_tail(publisher.as_ref(), &path, &content[..byte_offset])?;
                    repaired_torn_tail = true;
                    break;
                }
                Err(error) => return Err(invalid_format(line_number, error.to_string())),
            }
        }

        // 与 TypeScript Store 一致：完整但缺少结尾换行的旧文件在首次打开时
        // 修复，确保之后的 append 永远从新的物理行开始。
        if !repaired_torn_tail && !content.is_empty() && !content.ends_with('\n') {
            let mut file = publisher.append(&path)?;
            file.write_all(b"\n")?;
            file.sync_data()?;
        }

        Ok(Self {
            path,
            header,
            next_sequence: state.next_sequence(),
            mutations,
            state,
            publisher,
        })
    }

    /// 返回文件路径和不可变 header 的副本。
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn header(&self) -> &SessionHeader {
        &self.header
    }

    /// 返回已恢复的变更日志。
    pub fn mutations(&self) -> &[SessionMutation] {
        &self.mutations
    }

    /// 返回由 mutation 归约出的只读 Session 状态。
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// 追加一条变更。调用方不需要手动提供 sequence，Store 统一分配。
    pub fn append(
        &mut self,
        kind: impl Into<String>,
        payload: serde_json::Map<String, Value>,
    ) -> Result<SessionMutation, SessionStoreError> {
        let mutation = SessionMutation {
            kind: kind.into(),
            seq: self.next_sequence,
            payload,
        };
        if mutation.kind.is_empty() {
            return Err(SessionStoreError::InvalidArgument(
                "mutation kind cannot be empty".into(),
            ));
        }

        // 在落盘前对副本完成状态校验；写入失败或 mutation 无效时，真实状态
        // 均保持不变。写入成功后再应用到真实状态，保证磁盘与内存顺序一致。
        let mut candidate_state = self.state.clone();
        candidate_state.apply(&mutation)?;

        let mut file = self.publisher.append(&self.path)?;
        write_json_line(&mut file, &mutation)?;
        file.sync_data()?;

        self.state = candidate_state;
        self.mutations.push(mutation.clone());
        self.next_sequence = self.state.next_sequence();
        Ok(mutation)
    }

    /// 以 v4 的 provisioning 语义写入 entry。
    ///
    /// parent 固定为目标 lane 当前叶节点；sequence 与毫秒时间戳由 Store 统一
    /// 分配，避免不同调用方产生不连续日志或错误的分支连接。
    pub fn append_entry(&mut self, mut entry: NewEntry) -> Result<SessionEntry, SessionStoreError> {
        let parent_id = self
            .state
            .lane_leaf(&entry.lane)
            .ok_or_else(|| {
                SessionStoreError::InvalidMutation(format!("lane not found: {}", entry.lane))
            })?
            .map(str::to_owned);
        entry
            .fields
            .insert("lane".into(), Value::String(entry.lane));
        entry.fields.insert(
            "parentId".into(),
            parent_id.map_or(Value::Null, Value::String),
        );
        entry
            .fields
            .insert("timestamp".into(), Value::from(unix_millis()));
        entry.fields.remove("seq");

        let mutation = self.append("entry", entry.fields)?;
        let mut provisioned = mutation.payload;
        provisioned.insert("seq".into(), Value::from(mutation.seq));
        Ok(provisioned)
    }

    /// 以 v4 的 provisioning 语义写入 record。
    ///
    /// 同一 lane 不能同时启动多个 operation；这是 TypeScript Store 在持久化前
    /// 强制的串行运行约束。
    pub fn append_record(
        &mut self,
        mut record: NewRecord,
    ) -> Result<SessionRecord, SessionStoreError> {
        let payload_lane = record
            .fields
            .get("lane")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SessionStoreError::InvalidArgument("record fields must include string lane".into())
            })?;
        if payload_lane != record.lane {
            return Err(SessionStoreError::InvalidArgument(
                "record lane does not match fields.lane".into(),
            ));
        }
        if record.fields.get("type").and_then(Value::as_str) == Some("operation_started")
            && !self
                .state
                .find_open_operations(&record.lane, Some(1))?
                .is_empty()
        {
            return Err(SessionStoreError::InvalidMutation(format!(
                "lane {} already has an open operation",
                record.lane
            )));
        }
        record
            .fields
            .insert("timestamp".into(), Value::from(unix_millis()));
        record.fields.remove("seq");

        let mutation = self.append("record", record.fields)?;
        let mut provisioned = mutation.payload;
        provisioned.insert("seq".into(), Value::from(mutation.seq));
        Ok(provisioned)
    }

    /// 在指定位置创建新 lane。目标为空时创建空 lane；否则目标必须是已知 entry。
    pub fn create_lane(
        &mut self,
        lane: impl Into<String>,
        at: Option<&str>,
    ) -> Result<(), SessionStoreError> {
        let lane = lane.into();
        if lane.is_empty() {
            return Err(SessionStoreError::InvalidArgument(
                "lane cannot be empty".into(),
            ));
        }
        if self.state.has_lane(&lane) {
            return Err(SessionStoreError::InvalidMutation(format!(
                "lane already exists: {lane}"
            )));
        }
        self.append_lane_mutation(lane, at)
    }

    /// 将已有 lane 指向指定 entry，或重置为空 lane。
    pub fn move_lane(
        &mut self,
        lane: impl Into<String>,
        to: Option<&str>,
    ) -> Result<(), SessionStoreError> {
        let lane = lane.into();
        if !self.state.has_lane(&lane) {
            return Err(SessionStoreError::InvalidMutation(format!(
                "lane not found: {lane}"
            )));
        }
        self.append_lane_mutation(lane, to)
    }

    /// 写入或清除 Session 名称。
    pub fn set_name(&mut self, name: Option<&str>) -> Result<(), SessionStoreError> {
        let mut payload = serde_json::Map::new();
        payload.insert("fact".into(), Value::String("name".into()));
        payload.insert(
            "name".into(),
            name.map_or(Value::Null, |value| Value::String(value.to_owned())),
        );
        self.append("fact", payload).map(|_| ())
    }

    /// 写入或清除 entry 标签。
    pub fn set_label(
        &mut self,
        target_id: impl Into<String>,
        label: Option<&str>,
    ) -> Result<(), SessionStoreError> {
        let mut payload = serde_json::Map::new();
        payload.insert("fact".into(), Value::String("label".into()));
        payload.insert("targetId".into(), Value::String(target_id.into()));
        payload.insert(
            "label".into(),
            label.map_or(Value::Null, |value| Value::String(value.to_owned())),
        );
        self.append("fact", payload).map(|_| ())
    }

    fn append_lane_mutation(
        &mut self,
        lane: String,
        leaf_id: Option<&str>,
    ) -> Result<(), SessionStoreError> {
        let mut payload = serde_json::Map::new();
        payload.insert("lane".into(), Value::String(lane));
        payload.insert(
            "leafId".into(),
            leaf_id.map_or(Value::Null, |value| Value::String(value.to_owned())),
        );
        self.append("lane", payload).map(|_| ())
    }
    /// 将当前状态投影为新 Session 所需的连续 mutation 序列。
    ///
    /// 此处只负责复制范围和顺序，不负责目标文件创建或原子发布；后者属于
    /// repository 层。分离两者可使 JSONL、内存或未来其他存储后端复用相同的
    /// TypeScript branch/tree fork 语义。
    pub(crate) fn fork_mutations(
        &self,
        options: ForkOptions,
    ) -> Result<Vec<SessionMutation>, SessionStoreError> {
        let (copied_entries, lanes) = match options {
            ForkOptions::Tree => (
                self.state
                    .find_entries(EntryQuery {
                        oldest_first: true,
                        ..EntryQuery::default()
                    })?
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>(),
                self.state.lanes(),
            ),
            ForkOptions::Branch { entry_id, position } => {
                let explicit_entry_id = entry_id.is_some();
                let selected_id = match entry_id {
                    Some(id) => Some(id),
                    None => self.state.lane_leaf("main").flatten().map(str::to_owned),
                };
                let target_id = match selected_id {
                    None => None,
                    Some(id) => {
                        let entry = self.state.entry(&id).ok_or_else(|| {
                            SessionStoreError::InvalidArgument(format!(
                                "fork target not found: {id}"
                            ))
                        })?;
                        if entry.get("type").and_then(Value::as_str) != Some("message") {
                            return Err(SessionStoreError::InvalidArgument(format!(
                                "fork target is not a message entry: {id}"
                            )));
                        }
                        match position.unwrap_or(if explicit_entry_id {
                            ForkPosition::Before
                        } else {
                            ForkPosition::At
                        }) {
                            ForkPosition::At => Some(id),
                            ForkPosition::Before => entry
                                .get("parentId")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                        }
                    }
                };
                let entries = match target_id.as_deref() {
                    None => Vec::new(),
                    Some(id) => self
                        .state
                        .find_entries_on_branch(id, true, None)?
                        .into_iter()
                        .cloned()
                        .collect(),
                };
                (
                    entries,
                    vec![LanePointer {
                        lane: "main".into(),
                        leaf_id: target_id,
                    }],
                )
            }
        };

        let mut mutations = Vec::new();
        let mut sequence = 1;
        for mut entry in copied_entries.iter().cloned() {
            entry.remove("seq");
            mutations.push(SessionMutation {
                kind: "entry".into(),
                seq: sequence,
                payload: entry,
            });
            sequence += 1;
        }
        for lane in lanes {
            let mut payload = serde_json::Map::new();
            payload.insert("lane".into(), Value::String(lane.lane));
            payload.insert(
                "leafId".into(),
                lane.leaf_id.map_or(Value::Null, Value::String),
            );
            mutations.push(SessionMutation {
                kind: "lane".into(),
                seq: sequence,
                payload,
            });
            sequence += 1;
        }
        if let Some(name) = self.state.name() {
            let mut payload = serde_json::Map::new();
            payload.insert("fact".into(), Value::String("name".into()));
            payload.insert("name".into(), Value::String(name.to_owned()));
            mutations.push(SessionMutation {
                kind: "fact".into(),
                seq: sequence,
                payload,
            });
            sequence += 1;
        }
        for entry in copied_entries {
            let id = entry
                .get("id")
                .and_then(Value::as_str)
                .expect("validated entry id");
            if let Some(label) = self.state.label(id) {
                let mut payload = serde_json::Map::new();
                payload.insert("fact".into(), Value::String("label".into()));
                payload.insert("targetId".into(), Value::String(id.to_owned()));
                payload.insert("label".into(), Value::String(label.to_owned()));
                mutations.push(SessionMutation {
                    kind: "fact".into(),
                    seq: sequence,
                    payload,
                });
                sequence += 1;
            }
        }
        Ok(mutations)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkPosition {
    Before,
    At,
}

fn validate_header(header: &SessionHeader) -> Result<(), SessionStoreError> {
    if header.kind != "header" || header.version != FORMAT_VERSION {
        return Err(SessionStoreError::InvalidArgument(
            "unsupported session header".into(),
        ));
    }
    if header.id.is_empty() || header.cwd.is_empty() {
        return Err(SessionStoreError::InvalidArgument(
            "session header id and cwd must be non-empty".into(),
        ));
    }
    if header.parent_session_id.is_some() && header.legacy_parent_session_path.is_some() {
        return Err(SessionStoreError::InvalidArgument(
            "session header cannot contain both parentSessionId and legacyParentSessionPath".into(),
        ));
    }
    Ok(())
}

fn write_json_line<T: Serialize>(file: &mut File, value: &T) -> Result<(), SessionStoreError> {
    serde_json::to_writer(&mut *file, value)
        .map_err(|error| invalid_format(0, format!("failed to encode JSON: {error}")))?;
    file.write_all(b"\n")?;
    Ok(())
}

fn parse_line<T: for<'de> Deserialize<'de>>(
    line: &str,
    line_number: usize,
) -> Result<T, SessionStoreError> {
    serde_json::from_str(line).map_err(|error| invalid_format(line_number, error.to_string()))
}

fn invalid_format(line: usize, message: impl Into<String>) -> SessionStoreError {
    SessionStoreError::InvalidFormat {
        line,
        message: message.into(),
    }
}

/// 以完整有效前缀原子修复末尾 torn tail。
///
/// 临时文件与目标位于同一目录，保证 `rename` 不跨文件系统。原文件只会在临时
/// 文件已完整落盘后被替换；失败时尽力清理临时文件，但始终将原始 I/O 错误返回，
/// 从而保持与 TypeScript `publishFileAtomically()` 相同的故障语义。
fn repair_torn_tail(
    publisher: &dyn SessionFilePublisher,
    path: &Path,
    valid_prefix: &str,
) -> Result<(), SessionStoreError> {
    let temporary = PathBuf::from(format!("{}.tmp", path.to_string_lossy()));
    let result = (|| -> Result<(), SessionStoreError> {
        // 临时路径由本函数唯一拥有；若上次异常退出遗留文件，先清理后再以
        // create-new 打开，避免在未知内容上 truncate 后形成不可诊断的混合状态。
        if temporary.exists() {
            publisher.remove_file(&temporary)?;
        }
        let mut file = publisher.create_new(&temporary)?;
        file.write_all(valid_prefix.as_bytes())?;
        file.sync_all()?;
        drop(file);
        publisher.rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = publisher.remove_file(&temporary);
    }
    result
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::{
        JsonlSessionStore, SessionErrorCode, SessionFilePublisher, SessionStoreError,
        StandardSessionFilePublisher,
    };
    use serde_json::{Map, Value, json};
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    #[derive(Debug, Default)]
    struct RenameFailingPublisher {
        standard: StandardSessionFilePublisher,
    }

    impl SessionFilePublisher for RenameFailingPublisher {
        fn create_new(&self, path: &Path) -> io::Result<File> {
            self.standard.create_new(path)
        }

        fn append(&self, path: &Path) -> io::Result<File> {
            self.standard.append(path)
        }

        fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
            Err(io::Error::other("injected torn-tail rename failure"))
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.standard.remove_file(path)
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("session-{name}-{}.jsonl", std::process::id()))
    }

    fn entry_payload(id: &str, parent_id: Value) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("id".into(), json!(id));
        map.insert("type".into(), json!("message"));
        map.insert("parentId".into(), parent_id);
        map.insert("timestamp".into(), json!(1));
        map.insert("lane".into(), json!("main"));
        map
    }

    #[test]
    fn projects_every_public_error_category_to_the_typescript_code() {
        // 错误文本仅用于 Rust 内部诊断；跨语言调用方只依赖 `code()`。这里集中
        // 固化所有分类分支，避免后续修改诊断文案时悄然改变 JSONL API 契约。
        let cases = [
            (
                SessionStoreError::InvalidArgument("session not found: missing".into()),
                SessionErrorCode::NotFound,
            ),
            (
                SessionStoreError::InvalidMutation("duplicate session id: repeated".into()),
                SessionErrorCode::AlreadyExists,
            ),
            (
                SessionStoreError::SequenceGap {
                    expected: 2,
                    actual: 3,
                },
                SessionErrorCode::InvalidEntry,
            ),
            (
                SessionStoreError::InvalidMutation("field timestamp must be a string".into()),
                SessionErrorCode::InvalidPayload,
            ),
            (
                SessionStoreError::InvalidMutation("lane not found: review".into()),
                SessionErrorCode::InvalidLane,
            ),
            (
                SessionStoreError::InvalidArgument(
                    "operation kind requires record type operation_started".into(),
                ),
                SessionErrorCode::InvalidQuery,
            ),
            (
                SessionStoreError::InvalidArgument("fork target not found: missing".into()),
                SessionErrorCode::InvalidForkTarget,
            ),
            (
                SessionStoreError::InvalidFormat {
                    line: 2,
                    message: "malformed JSON".into(),
                },
                SessionErrorCode::Storage,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.code(), expected);
        }
    }

    #[test]
    fn serializes_header_with_the_typescript_v4_field_names() {
        let path = temp_path("header-camel-case");
        let _ = fs::remove_file(&path);
        let store = JsonlSessionStore::create(&path, "session", "C:/work").unwrap();
        let header = serde_json::to_value(store.header()).unwrap();
        assert!(header.get("createdAt").is_some());
        assert!(header.get("created_at").is_none());
        assert!(header.get("parentSessionId").is_none());
        drop(store);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn create_append_and_reopen_preserves_sequence() {
        let path = temp_path("roundtrip");
        let _ = fs::remove_file(&path);
        let mut store = JsonlSessionStore::create(&path, "session", ".").unwrap();
        assert_eq!(
            store
                .append("entry", entry_payload("first", Value::Null))
                .unwrap()
                .seq,
            1
        );
        let mut fact = Map::new();
        fact.insert("fact".into(), json!("name"));
        fact.insert("name".into(), json!("demo"));
        assert_eq!(store.append("fact", fact).unwrap().seq, 2);
        assert_eq!(store.state().name(), Some("demo"));
        drop(store);

        let reopened = JsonlSessionStore::open(&path).unwrap();
        assert_eq!(reopened.header().id, "session");
        assert_eq!(reopened.mutations().len(), 2);
        assert_eq!(reopened.state().name(), Some("demo"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn lane_and_label_helpers_write_v4_mutations() {
        let path = temp_path("lane-and-label");
        let _ = fs::remove_file(&path);
        let mut store = JsonlSessionStore::create(&path, "session", ".").unwrap();
        store
            .append("entry", entry_payload("root", Value::Null))
            .unwrap();
        store.create_lane("review", Some("root")).unwrap();
        store.move_lane("review", None).unwrap();
        store.set_label("root", Some("keep")).unwrap();
        assert_eq!(store.state().lane_leaf("review"), Some(None));
        assert_eq!(store.state().label("root"), Some("keep"));
        assert!(matches!(
            store.create_lane("review", None),
            Err(SessionStoreError::InvalidMutation(_))
        ));
        drop(store);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn repairs_an_incomplete_tail_atomically_without_losing_valid_prefix() {
        let path = temp_path("torn-tail");
        let temporary = PathBuf::from(format!("{}.tmp", path.to_string_lossy()));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&temporary);
        let mut store = JsonlSessionStore::create(&path, "session", ".").unwrap();
        store
            .append("entry", entry_payload("first", Value::Null))
            .unwrap();
        drop(store);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"kind\":\"entry\",\"seq\":2").unwrap();
        drop(file);

        let reopened = JsonlSessionStore::open(&path).unwrap();
        assert_eq!(reopened.mutations().len(), 1);
        assert!(!temporary.exists(), "成功发布后不应遗留暂存文件");
        let repaired = fs::read_to_string(&path).unwrap();
        assert!(repaired.ends_with('\n'));
        assert!(!repaired.contains("\"seq\":2"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn preserves_original_session_when_torn_tail_staging_fails() {
        let path = temp_path("torn-tail-staging-failure");
        let temporary = PathBuf::from(format!("{}.tmp", path.to_string_lossy()));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_dir(&temporary);
        let mut store = JsonlSessionStore::create(&path, "session", ".").unwrap();
        store
            .append("entry", entry_payload("first", Value::Null))
            .unwrap();
        drop(store);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"kind\":\"entry\",\"seq\":2").unwrap();
        drop(file);
        let original = fs::read_to_string(&path).unwrap();

        // 用同名目录稳定模拟暂存文件无法创建。此处验证失败时不能触碰目标文件，
        // 对齐 TypeScript `publishFileAtomically()` 的故障保留语义。
        fs::create_dir(&temporary).unwrap();
        assert!(matches!(
            JsonlSessionStore::open(&path),
            Err(SessionStoreError::Io(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);

        fs::remove_dir(temporary).unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn preserves_original_session_when_torn_tail_rename_fails() {
        let path = temp_path("torn-tail-rename-failure");
        let temporary = PathBuf::from(format!("{}.tmp", path.to_string_lossy()));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&temporary);
        let mut store = JsonlSessionStore::create(&path, "session", ".").unwrap();
        store
            .append("entry", entry_payload("first", Value::Null))
            .unwrap();
        drop(store);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"kind\":\"entry\",\"seq\":2").unwrap();
        drop(file);
        let original = fs::read(&path).unwrap();

        let error = JsonlSessionStore::open_with_publisher(
            &path,
            Arc::new(RenameFailingPublisher::default()),
        )
        .unwrap_err();
        assert_eq!(error.code(), SessionErrorCode::Storage);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(!temporary.exists(), "rename 失败后必须清理完整暂存文件");

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn completes_a_valid_final_line_missing_its_newline() {
        let path = temp_path("missing-final-newline");
        let _ = fs::remove_file(&path);
        let mut store = JsonlSessionStore::create(&path, "session", ".").unwrap();
        store
            .append("entry", entry_payload("first", Value::Null))
            .unwrap();
        drop(store);
        let mut content = fs::read_to_string(&path).unwrap();
        content.pop();
        fs::write(&path, content).unwrap();

        let reopened = JsonlSessionStore::open(&path).unwrap();
        assert_eq!(reopened.mutations().len(), 1);
        assert!(fs::read_to_string(&path).unwrap().ends_with('\n'));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn preserves_a_complete_invalid_final_mutation() {
        let path = temp_path("complete-invalid-tail");
        let _ = fs::remove_file(&path);
        let mut store = JsonlSessionStore::create(&path, "session", ".").unwrap();
        store
            .append("entry", entry_payload("first", Value::Null))
            .unwrap();
        drop(store);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{\"kind\":\"fact\",\"seq\":2}}").unwrap();
        drop(file);
        let original = fs::read_to_string(&path).unwrap();

        // 语法完整但不符合 mutation schema 的末行不是 torn tail。必须保留
        // 原始字节，供调用方报错、重试或人工处理，而不能误删真实数据。
        assert!(matches!(
            JsonlSessionStore::open(&path),
            Err(SessionStoreError::InvalidFormat { line: 3, .. })
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_malformed_mutation_before_the_last_physical_line() {
        let path = temp_path("middle-corruption");
        let _ = fs::remove_file(&path);
        let mut store = JsonlSessionStore::create(&path, "session", ".").unwrap();
        store
            .append("entry", entry_payload("first", Value::Null))
            .unwrap();
        drop(store);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{\"kind\":\"entry\",\"seq\":2").unwrap();
        writeln!(file, "{{\"kind\":\"fact\",\"seq\":3}}").unwrap();
        drop(file);

        let error = JsonlSessionStore::open(&path).unwrap_err();
        assert!(matches!(
            error,
            SessionStoreError::InvalidFormat { line: 3, .. }
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_sequence_gaps() {
        let path = temp_path("sequence-gap");
        let _ = fs::remove_file(&path);
        let mut store = JsonlSessionStore::create(&path, "session", ".").unwrap();
        store
            .append("entry", entry_payload("first", Value::Null))
            .unwrap();
        drop(store);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{\"kind\":\"entry\",\"seq\":3,\"payload\":{{}}}}").unwrap();
        drop(file);

        let error = JsonlSessionStore::open(&path).unwrap_err();
        assert!(matches!(error, SessionStoreError::SequenceGap { .. }));
        fs::remove_file(path).unwrap();
    }
}
