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

mod state;

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

/// Session Store 的统一错误。
#[derive(Debug)]
pub enum SessionStoreError {
    Io(io::Error),
    InvalidFormat { line: usize, message: String },
    InvalidArgument(String),
    InvalidMutation(String),
    SequenceGap { expected: u64, actual: u64 },
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
}

impl JsonlSessionStore {
    /// 创建一个新文件并写入 v4 header；目标文件已存在时拒绝覆盖。
    pub fn create(
        path: impl Into<PathBuf>,
        id: impl Into<String>,
        cwd: impl Into<String>,
    ) -> Result<Self, SessionStoreError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(SessionStoreError::InvalidArgument(
                "session path cannot be empty".into(),
            ));
        }
        if path.exists() {
            return Err(SessionStoreError::InvalidArgument(format!(
                "session already exists: {}",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let header = SessionHeader {
            kind: "header".into(),
            version: FORMAT_VERSION,
            id: id.into(),
            created_at: unix_millis(),
            cwd: cwd.into(),
            parent_session_id: None,
            legacy_parent_session_path: None,
            metadata: None,
        };
        let mut file = File::create(&path)?;
        write_json_line(&mut file, &header)?;
        file.sync_all()?;

        Ok(Self {
            path,
            header,
            mutations: Vec::new(),
            state: SessionState::new(),
            next_sequence: 1,
        })
    }

    /// 从已有 JSONL 文件恢复内存索引。
    ///
    /// 最后一行如果是未完成的 JSON（典型的进程中断写入），只截断到最后
    /// 一个完整换行符；中间位置的损坏则直接失败，避免悄悄丢失历史数据。
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, SessionStoreError> {
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
        if header.kind != "header" || header.version != FORMAT_VERSION {
            return Err(invalid_format(1, "unsupported session header"));
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
                    truncate_file(&path, byte_offset)?;
                    repaired_torn_tail = true;
                    break;
                }
                Err(error) => return Err(invalid_format(line_number, error.to_string())),
            }
        }

        // 与 TypeScript Store 一致：完整但缺少结尾换行的旧文件在首次打开时
        // 修复，确保之后的 append 永远从新的物理行开始。
        if !repaired_torn_tail && !content.is_empty() && !content.ends_with('\n') {
            let mut file = OpenOptions::new().append(true).open(&path)?;
            file.write_all(b"\n")?;
            file.sync_data()?;
        }

        Ok(Self {
            path,
            header,
            next_sequence: state.next_sequence(),
            mutations,
            state,
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

        let mut file = OpenOptions::new().append(true).open(&self.path)?;
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

fn truncate_file(path: &Path, length: usize) -> Result<(), SessionStoreError> {
    let file = OpenOptions::new().write(true).open(path)?;
    file.set_len(length as u64)?;
    file.sync_all()?;
    Ok(())
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::{JsonlSessionStore, SessionStoreError};
    use serde_json::{Map, Value, json};
    use std::fs::{self, OpenOptions};
    use std::io::Write;

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
    fn repairs_an_incomplete_tail_without_losing_valid_prefix() {
        let path = temp_path("torn-tail");
        let _ = fs::remove_file(&path);
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
