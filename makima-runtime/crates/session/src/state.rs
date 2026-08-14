//! JSONL v4 mutation 的内存状态机。
//!
//! 此模块不执行文件 I/O。它只验证 mutation 的时序和引用关系，并维护
//! Session 的最小索引。将状态归约与持久化拆分后，文件加载、追加写入和未来
//! 的 RPC/actor 实现能够复用完全相同的业务校验。

// 状态归约与查询

// 将 append-only 日志归约成当前状态，见 SessionState。
// 查询 entries、records、分支链、开放 operation、完整 log、lane 指针、标签，以及 token/cost 统计，见 SessionState。

use crate::{SessionMutation, SessionStoreError};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// 可用于调用方展示或构造 fork 的 entry 原始 JSON 数据。
pub type SessionEntry = Map<String, Value>;

/// 可用于读取 API 的 record 原始 JSON 数据。
pub type SessionRecord = Map<String, Value>;

/// 供读取 API 使用的 entry 查询条件。省略顺序时按 TypeScript Store 的默认值
/// 返回最新 entry；`after_sequence` 始终是排他的 sequence 游标。
#[derive(Debug, Clone, Default)]
pub struct EntryQuery<'a> {
    pub entry_type: Option<&'a str>,
    pub custom_type: Option<&'a str>,
    pub oldest_first: bool,
    pub limit: Option<usize>,
    pub after_sequence: Option<u64>,
}

/// 供读取 API 使用的 record 查询条件。默认按最新 record 返回；
/// `after_sequence` 无论排序方向均表示严格大于该 sequence。
#[derive(Debug, Clone, Default)]
pub struct RecordQuery<'a> {
    pub lane: Option<&'a str>,
    pub record_type: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub operation_kind: Option<&'a str>,
    pub oldest_first: bool,
    pub limit: Option<usize>,
    pub after_sequence: Option<u64>,
}

/// 分支扫描边界。命中边界 entry 后，该 entry 仍会包含在结果中。
#[derive(Debug, Clone, Default)]
pub struct BranchBounds<'a> {
    pub stop_at_id: Option<&'a str>,
    pub stop_at_type: Option<&'a str>,
}

/// 完整 mutation 日志的增量读取条件。日志始终按 sequence 正序返回。
#[derive(Debug, Clone, Copy, Default)]
pub struct LogOptions {
    pub limit: Option<usize>,
    pub after_sequence: Option<u64>,
}

/// Session 累计用量。字段名称和 TypeScript `SessionStats` 保持一致的语义。
///
/// Provider 可以追加负数 usage 作为计费或 token 用量更正，因此 token 统计不能
/// 使用无符号整数；否则 TypeScript 能恢复的合法 correction record 会被 Rust
/// 拒绝，或在累计时产生溢出。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SessionStats {
    pub message_count: u64,
    pub cached_tokens: i64,
    pub uncached_tokens: i64,
    pub total_tokens: i64,
    pub cost_total: f64,
}

/// Lane 的稳定快照。保持 `Vec` 而非暴露内部 HashMap，避免调用方依赖内部索引。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanePointer {
    pub lane: String,
    pub leaf_id: Option<String>,
}

/// Session v4 mutation 的最小内存投影。
///
/// Entry 和 record 的具体业务字段由 TypeScript Provider/Extension 维护；Rust
/// 在当前阶段只校验所有类型共有的身份、sequence、lane 与 parent 关系，避免
/// 因复制 Provider 消息联合类型而把不稳定的上层协议固化进 runtime。
#[derive(Debug, Clone, Default)]
pub struct SessionState {
    sequence: u64,
    used_ids: HashSet<String>,
    entries: Vec<SessionEntry>,
    entry_positions: HashMap<String, usize>,
    records: Vec<SessionRecord>,
    open_operation_positions: HashMap<String, HashMap<String, usize>>,
    log: Vec<SessionMutation>,
    lanes: HashMap<String, Option<String>>,
    lane_order: Vec<String>,
    name: Option<String>,
    labels: HashMap<String, String>,
    stats: SessionStats,
}

impl SessionState {
    /// 创建包含 TypeScript 默认 `main` lane 的空状态。
    pub fn new() -> Self {
        let mut lanes = HashMap::new();
        lanes.insert("main".to_owned(), None);
        Self {
            lanes,
            lane_order: vec!["main".to_owned()],
            ..Self::default()
        }
    }

    /// 下一个必须出现的全局 sequence。
    pub fn next_sequence(&self) -> u64 {
        self.sequence + 1
    }

    /// 获取一个 entry 的不可变 JSON 视图。
    pub fn entry(&self, id: &str) -> Option<&SessionEntry> {
        self.entry_positions
            .get(id)
            .and_then(|position| self.entries.get(*position))
    }

    /// 获取 lane 当前叶节点。不存在的 lane 返回 `None`，与空 lane 区分。
    pub fn lane_leaf(&self, lane: &str) -> Option<Option<&str>> {
        self.lanes.get(lane).map(|leaf_id| leaf_id.as_deref())
    }

    /// 返回 lane 是否存在，用于区分不存在和叶节点为 `null` 的空 lane。
    pub fn has_lane(&self, lane: &str) -> bool {
        self.lanes.contains_key(lane)
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn label(&self, entry_id: &str) -> Option<&str> {
        self.labels.get(entry_id).map(String::as_str)
    }

    /// 获取 lane 快照，保持首次出现的 insertion order，与 TypeScript `Map` 语义一致。
    pub fn lanes(&self) -> Vec<LanePointer> {
        self.lane_order
            .iter()
            .filter_map(|lane| {
                self.lanes.get(lane).map(|leaf_id| LanePointer {
                    lane: lane.clone(),
                    leaf_id: leaf_id.clone(),
                })
            })
            .collect()
    }

    /// 在全局 entry 日志中执行最小 v4 查询。
    pub fn find_entries(
        &self,
        query: EntryQuery<'_>,
    ) -> Result<Vec<&SessionEntry>, SessionStoreError> {
        validate_limit(query.limit)?;
        validate_cursor(query.after_sequence)?;
        let mut entries: Vec<&SessionEntry> = self.entries.iter().collect();
        if !query.oldest_first {
            entries.reverse();
        }
        Ok(entries
            .into_iter()
            .filter(|entry| entry_matches(entry, &query))
            .take(query.limit.unwrap_or(usize::MAX))
            .collect())
    }

    /// 从指定 entry 沿 parent chain 回溯。默认返回 leaf 到 root；传入
    /// `oldest_first` 后反转为 root 到 leaf，便于 fork 和上下文重建。
    pub fn find_entries_on_branch(
        &self,
        start: &str,
        oldest_first: bool,
        limit: Option<usize>,
    ) -> Result<Vec<&SessionEntry>, SessionStoreError> {
        self.find_entries_on_branch_with_bounds(
            start,
            EntryQuery {
                oldest_first,
                limit,
                ..EntryQuery::default()
            },
            BranchBounds::default(),
        )
    }

    /// 在 parent chain 上执行带筛选条件和停止边界的分支查询。
    pub fn find_entries_on_branch_with_bounds(
        &self,
        start: &str,
        query: EntryQuery<'_>,
        bounds: BranchBounds<'_>,
    ) -> Result<Vec<&SessionEntry>, SessionStoreError> {
        validate_limit(query.limit)?;
        validate_cursor(query.after_sequence)?;
        // TypeScript 的 `walkToRoot()` 只会在 newest-first 查询时接收边界；
        // oldest-first 则先读取完整 parent chain、反转为 root -> leaf，再停止。
        // 这看似反直觉，但它决定了 root -> middle -> leaf 且 stopAt=middle 时
        // 两种顺序分别返回 [leaf, middle] 和 [root, middle]，必须逐字对齐。
        let mut branch = Vec::new();
        let mut current_id = Some(start);
        let mut visited = HashSet::new();
        while let Some(entry_id) = current_id {
            if !visited.insert(entry_id) {
                return Err(invalid(format!(
                    "session branch contains a cycle at: {entry_id}"
                )));
            }
            let entry = self
                .entry(entry_id)
                .ok_or_else(|| invalid(format!("entry not found: {entry_id}")))?;
            let reached_bound = entry.get("id").and_then(Value::as_str) == bounds.stop_at_id
                || entry.get("type").and_then(Value::as_str) == bounds.stop_at_type;
            branch.push(entry);
            if !query.oldest_first && reached_bound {
                break;
            }
            current_id = nullable_string(entry, "parentId")?;
        }
        if query.oldest_first {
            branch.reverse();
        }

        let mut result = Vec::new();
        for entry in branch {
            let reached_bound = entry.get("id").and_then(Value::as_str) == bounds.stop_at_id
                || entry.get("type").and_then(Value::as_str) == bounds.stop_at_type;
            if entry_matches(entry, &query) {
                result.push(entry);
            }
            if (query.oldest_first && reached_bound)
                || result.len() == query.limit.unwrap_or(usize::MAX)
            {
                break;
            }
        }
        Ok(result)
    }

    /// 在全局 record 日志中执行最小 v4 查询。
    pub fn find_records(
        &self,
        query: RecordQuery<'_>,
    ) -> Result<Vec<&SessionRecord>, SessionStoreError> {
        validate_record_query(&query)?;
        let mut records: Vec<&SessionRecord> = self.records.iter().collect();
        if !query.oldest_first {
            records.reverse();
        }
        Ok(records
            .into_iter()
            .filter(|record| record_matches(record, &query))
            .take(query.limit.unwrap_or(usize::MAX))
            .collect())
    }

    /// 返回某 lane 尚未结束的操作，默认按最新 operation_started record 优先。
    pub fn find_open_operations(
        &self,
        lane: &str,
        limit: Option<usize>,
    ) -> Result<Vec<&SessionRecord>, SessionStoreError> {
        validate_limit(limit)?;
        let Some(positions) = self.open_operation_positions.get(lane) else {
            return Ok(Vec::new());
        };
        let mut positions: Vec<_> = positions.values().copied().collect();
        positions.sort_unstable_by(|left, right| right.cmp(left));
        Ok(positions
            .into_iter()
            .filter_map(|position| self.records.get(position))
            .take(limit.unwrap_or(usize::MAX))
            .collect())
    }

    /// 返回按 sequence 正序排列的完整 mutation 日志，可使用排他游标增量读取。
    pub fn get_log(&self, options: LogOptions) -> Result<Vec<&SessionMutation>, SessionStoreError> {
        validate_limit(options.limit)?;
        validate_cursor(options.after_sequence)?;
        Ok(self
            .log
            .iter()
            .filter(|mutation| {
                options
                    .after_sequence
                    .is_none_or(|after| mutation.seq > after)
            })
            .take(options.limit.unwrap_or(usize::MAX))
            .collect())
    }

    /// 返回累计统计；调用方得到副本，不能修改 Store 内部状态。
    pub fn stats(&self) -> SessionStats {
        self.stats
    }

    /// 应用已解码的 JSONL mutation。
    ///
    /// 所有变更先在局部变量中验证，只有验证完成后才会修改状态，调用方可以
    /// 安全地在写入前执行本方法的副本检查，或在加载失败后报告原始文件行号。
    pub fn apply(&mut self, mutation: &SessionMutation) -> Result<(), SessionStoreError> {
        if mutation.seq != self.next_sequence() {
            return Err(SessionStoreError::SequenceGap {
                expected: self.next_sequence(),
                actual: mutation.seq,
            });
        }

        match mutation.kind.as_str() {
            "entry" => self.apply_entry(mutation)?,
            "record" => self.apply_record(mutation)?,
            "lane" => self.apply_lane(mutation)?,
            "fact" => self.apply_fact(mutation)?,
            other => {
                return Err(SessionStoreError::InvalidMutation(format!(
                    "unsupported mutation kind: {other}"
                )));
            }
        }
        self.sequence = mutation.seq;
        self.log.push(mutation.clone());
        Ok(())
    }

    fn apply_entry(&mut self, mutation: &SessionMutation) -> Result<(), SessionStoreError> {
        let id = required_string(&mutation.payload, "id")?;
        let parent_id = nullable_string(&mutation.payload, "parentId")?;
        let entry_type = required_string(&mutation.payload, "type")?;
        require_timestamp(&mutation.payload)?;
        if !matches!(
            entry_type,
            "message"
                | "model_change"
                | "thinking_level_change"
                | "active_tools_change"
                | "compaction"
                | "branch_summary"
                | "custom"
        ) {
            return Err(invalid(format!("unsupported entry type: {entry_type}")));
        }
        self.ensure_unused_id(id)?;
        if entry_type == "custom" {
            required_string(&mutation.payload, "customType")?;
        }

        if let Some(lane) = optional_string(&mutation.payload, "lane")? {
            let leaf_id = self
                .lanes
                .get(lane)
                .ok_or_else(|| invalid(format!("entry references missing lane: {lane}")))?;
            if leaf_id.as_deref() != parent_id {
                return Err(invalid("entry does not chain to the lane leaf"));
            }
        }
        if let Some(parent_id) = parent_id
            && !self.entry_positions.contains_key(parent_id)
        {
            return Err(invalid(format!(
                "entry references missing parent: {parent_id}"
            )));
        }

        // JSONL 将 entry 的 sequence 平铺在 mutation 顶层；内存查询仍需要把
        // 它视为 entry 的固有字段，才能实现与 TypeScript cursor 相同的语义。
        let mut entry = mutation.payload.clone();
        entry.insert("seq".to_owned(), Value::from(mutation.seq));

        let position = self.entries.len();
        self.used_ids.insert(id.to_owned());
        self.entry_positions.insert(id.to_owned(), position);
        self.entries.push(entry);
        if entry_type == "message" {
            self.stats.message_count += 1;
        }
        if let Some(lane) = optional_string(&mutation.payload, "lane")? {
            self.lanes.insert(lane.to_owned(), Some(id.to_owned()));
        }
        Ok(())
    }

    fn apply_record(&mut self, mutation: &SessionMutation) -> Result<(), SessionStoreError> {
        let id = required_string(&mutation.payload, "id")?;
        let lane = required_string(&mutation.payload, "lane")?;
        let record_type = required_string(&mutation.payload, "type")?;
        require_timestamp(&mutation.payload)?;
        if !matches!(
            record_type,
            "operation_started"
                | "abort_requested"
                | "operation_finished"
                | "step_attempt"
                | "tool_started"
                | "queue_enqueued"
                | "queue_cancelled"
                | "write_deferred"
                | "usage"
        ) {
            return Err(invalid(format!("unsupported record type: {record_type}")));
        }
        if !self.lanes.contains_key(lane) {
            return Err(invalid(format!("record references missing lane: {lane}")));
        }
        self.ensure_unused_id(id)?;
        if record_type == "operation_started" {
            let intent = mutation
                .payload
                .get("intent")
                .and_then(Value::as_object)
                .ok_or_else(|| invalid("operation_started intent must be an object"))?;
            let operation_kind = required_string(intent, "kind")?;
            if !matches!(operation_kind, "run" | "compaction" | "navigation") {
                return Err(invalid(format!(
                    "unsupported operation intent kind: {operation_kind}"
                )));
            }
        }
        if record_type == "operation_finished" {
            required_string(&mutation.payload, "runId")?;
        }
        if matches!(record_type, "step_attempt" | "tool_started") {
            required_string(&mutation.payload, "runId")?;
        }
        if record_type == "usage" {
            self.apply_usage(&mutation.payload)?;
        }

        let mut record = mutation.payload.clone();
        record.insert("seq".to_owned(), Value::from(mutation.seq));
        let position = self.records.len();
        self.used_ids.insert(id.to_owned());
        self.records.push(record);
        if record_type == "operation_started" {
            self.open_operation_positions
                .entry(lane.to_owned())
                .or_default()
                .insert(id.to_owned(), position);
        } else if record_type == "operation_finished" {
            let run_id = required_string(&mutation.payload, "runId")?;
            if let Some(open_operations) = self.open_operation_positions.get_mut(lane) {
                open_operations.remove(run_id);
            }
        }
        Ok(())
    }

    fn apply_lane(&mut self, mutation: &SessionMutation) -> Result<(), SessionStoreError> {
        let lane = required_string(&mutation.payload, "lane")?;
        let leaf_id = nullable_string(&mutation.payload, "leafId")?;
        if let Some(leaf_id) = leaf_id
            && !self.entry_positions.contains_key(leaf_id)
        {
            return Err(invalid(format!(
                "lane references missing target: {leaf_id}"
            )));
        }
        if !self.lanes.contains_key(lane) {
            self.lane_order.push(lane.to_owned());
        }
        self.lanes
            .insert(lane.to_owned(), leaf_id.map(str::to_owned));
        Ok(())
    }

    fn apply_fact(&mut self, mutation: &SessionMutation) -> Result<(), SessionStoreError> {
        match required_string(&mutation.payload, "fact")? {
            "name" => {
                self.name = optional_string(&mutation.payload, "name")?.map(str::to_owned);
                Ok(())
            }
            "label" => {
                let target_id = required_string(&mutation.payload, "targetId")?;
                if !self.entry_positions.contains_key(target_id) {
                    return Err(invalid(format!(
                        "label references missing entry: {target_id}"
                    )));
                }
                match optional_string(&mutation.payload, "label")? {
                    Some(label) => {
                        self.labels.insert(target_id.to_owned(), label.to_owned());
                    }
                    None => {
                        self.labels.remove(target_id);
                    }
                }
                Ok(())
            }
            fact => Err(invalid(format!("unsupported fact: {fact}"))),
        }
    }

    fn apply_usage(&mut self, payload: &Map<String, Value>) -> Result<(), SessionStoreError> {
        let usage = payload
            .get("usage")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("usage record must contain an object field: usage"))?;
        let cost = usage
            .get("cost")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("usage record must contain an object field: usage.cost"))?;

        // 先验证和提取全部字段，再更新累计值。这样 malformed usage 不会只更新
        // 前几个计数器，保持 `apply()` 失败前后 state 的原子性。
        let cache_read = required_i64(usage, "cacheRead")?;
        let input = required_i64(usage, "input")?;
        let cache_write = required_i64(usage, "cacheWrite")?;
        let total_tokens = required_i64(usage, "totalTokens")?;
        let cost_total = required_f64(cost, "total")?;

        self.stats.cached_tokens += cache_read;
        self.stats.uncached_tokens += input + cache_write;
        self.stats.total_tokens += total_tokens;
        self.stats.cost_total += cost_total;
        Ok(())
    }

    fn ensure_unused_id(&self, id: &str) -> Result<(), SessionStoreError> {
        if self.used_ids.contains(id) {
            return Err(invalid(format!("duplicate session id: {id}")));
        }
        Ok(())
    }
}

fn required_string<'a>(
    payload: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, SessionStoreError> {
    optional_string(payload, field)?
        .ok_or_else(|| invalid(format!("missing required string field: {field}")))
}

fn optional_string<'a>(
    payload: &'a Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, SessionStoreError> {
    match payload.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(invalid(format!("field {field} must be a string"))),
    }
}

fn nullable_string<'a>(
    payload: &'a Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, SessionStoreError> {
    if !payload.contains_key(field) {
        return Err(invalid(format!(
            "missing required nullable string field: {field}"
        )));
    }
    optional_string(payload, field)
}

/// 校验 record 查询中不能由筛选逻辑“静默忽略”的组合。
///
/// `operationKind` 只属于 `operation_started`。若将它与其他 record 类型混用，
/// TypeScript 会在读取空日志前返回 `invalid_query`；必须在过滤前失败，不能因为
/// 当前没有记录而错误地返回空数组。
fn validate_record_query(query: &RecordQuery<'_>) -> Result<(), SessionStoreError> {
    validate_limit(query.limit)?;
    validate_cursor(query.after_sequence)?;
    if query.operation_kind.is_some() && query.record_type != Some("operation_started") {
        return Err(SessionStoreError::InvalidArgument(
            "operation kind requires record type operation_started".into(),
        ));
    }
    Ok(())
}

fn validate_limit(limit: Option<usize>) -> Result<(), SessionStoreError> {
    if limit == Some(0) {
        return Err(invalid("limit must be a positive integer"));
    }
    Ok(())
}

fn validate_cursor(cursor: Option<u64>) -> Result<(), SessionStoreError> {
    if cursor.is_some_and(|value| value > i64::MAX as u64) {
        return Err(invalid("cursor sequence must be a non-negative integer"));
    }
    Ok(())
}

fn entry_matches(entry: &SessionEntry, query: &EntryQuery<'_>) -> bool {
    let entry_type = entry.get("type").and_then(Value::as_str);
    let sequence = entry.get("seq").and_then(Value::as_u64);
    query
        .entry_type
        .is_none_or(|expected| entry_type == Some(expected))
        && query.custom_type.is_none_or(|expected| {
            entry_type == Some("custom")
                && entry.get("customType").and_then(Value::as_str) == Some(expected)
        })
        && query.after_sequence.is_none_or(|after_sequence| {
            sequence.is_some_and(|sequence| {
                if query.oldest_first {
                    sequence > after_sequence
                } else {
                    sequence < after_sequence
                }
            })
        })
}

fn record_matches(record: &SessionRecord, query: &RecordQuery<'_>) -> bool {
    let record_type = record.get("type").and_then(Value::as_str);
    let run_id_matches = query.run_id.is_none_or(|expected| {
        (record_type == Some("operation_started")
            && record.get("id").and_then(Value::as_str) == Some(expected))
            || record.get("runId").and_then(Value::as_str) == Some(expected)
    });
    let operation_kind_matches = query.operation_kind.is_none_or(|expected| {
        record_type == Some("operation_started")
            && record
                .get("intent")
                .and_then(Value::as_object)
                .and_then(|intent| intent.get("kind"))
                .and_then(Value::as_str)
                == Some(expected)
    });

    query
        .lane
        .is_none_or(|expected| record.get("lane").and_then(Value::as_str) == Some(expected))
        && query
            .record_type
            .is_none_or(|expected| record_type == Some(expected))
        && run_id_matches
        && operation_kind_matches
        && query.after_sequence.is_none_or(|after_sequence| {
            record
                .get("seq")
                .and_then(Value::as_u64)
                .is_some_and(|sequence| sequence > after_sequence)
        })
}

fn required_i64(payload: &Map<String, Value>, field: &str) -> Result<i64, SessionStoreError> {
    payload
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid(format!("field {field} must be an integer")))
}

fn required_f64(payload: &Map<String, Value>, field: &str) -> Result<f64, SessionStoreError> {
    payload
        .get(field)
        .and_then(Value::as_f64)
        .ok_or_else(|| invalid(format!("field {field} must be a number")))
}

fn require_timestamp(payload: &Map<String, Value>) -> Result<(), SessionStoreError> {
    match payload.get("timestamp") {
        Some(Value::Number(value)) if value.as_u64().is_some() => Ok(()),
        _ => Err(invalid("timestamp must be a non-negative integer")),
    }
}

fn invalid(message: impl Into<String>) -> SessionStoreError {
    SessionStoreError::InvalidMutation(message.into())
}

#[cfg(test)]
mod tests {
    use super::SessionState;
    use crate::SessionMutation;
    use serde_json::json;

    fn mutation(kind: &str, seq: u64, value: serde_json::Value) -> SessionMutation {
        let mut payload = value.as_object().unwrap().clone();
        payload.remove("kind");
        payload.remove("seq");
        SessionMutation {
            kind: kind.to_owned(),
            seq,
            payload,
        }
    }

    #[test]
    fn tracks_lanes_entries_and_facts_with_v4_shapes() {
        let mut state = SessionState::new();
        state
            .apply(&mutation(
                "entry",
                1,
                json!({"id":"root","type":"message","parentId":null,"timestamp":1,"lane":"main"}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "lane",
                2,
                json!({"lane":"review","leafId":"root"}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "entry",
                3,
                json!({"id":"reply","type":"message","parentId":"root","timestamp":2,"lane":"review"}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "fact",
                4,
                json!({"fact":"label","targetId":"reply","label":"keep"}),
            ))
            .unwrap();

        assert_eq!(state.lane_leaf("review"), Some(Some("reply")));
        assert_eq!(
            state.entry("root").unwrap().get("type"),
            Some(&json!("message"))
        );
        assert_eq!(state.label("reply"), Some("keep"));
    }

    #[test]
    fn accepts_records_only_for_existing_lanes() {
        let mut state = SessionState::new();
        state
            .apply(&mutation(
                "record",
                1,
                json!({"id":"run-1","type":"operation_started","lane":"main","timestamp":1,"intent":{"kind":"run"}}),
            ))
            .unwrap();
        let error = state
            .apply(&mutation(
                "record",
                2,
                json!({"id":"run-2","type":"operation_started","lane":"missing","timestamp":2,"intent":{"kind":"run"}}),
            ))
            .unwrap_err();
        assert!(error.to_string().contains("missing lane"));
    }

    #[test]
    fn rejects_entries_without_a_valid_timestamp() {
        let mut state = SessionState::new();
        let error = state
            .apply(&mutation(
                "entry",
                1,
                json!({"id":"root","type":"message","parentId":null,"lane":"main"}),
            ))
            .unwrap_err();
        assert!(error.to_string().contains("timestamp"));
    }

    #[test]
    fn rejects_entry_that_does_not_chain_to_its_lane_leaf() {
        let mut state = SessionState::new();
        let error = state
            .apply(&mutation(
                "entry",
                1,
                json!({"id":"orphan","type":"message","parentId":"missing","timestamp":1,"lane":"main"}),
            ))
            .unwrap_err();
        assert!(error.to_string().contains("does not chain"));
    }

    #[test]
    fn queries_entries_by_type_order_and_exclusive_cursor() {
        let mut state = SessionState::new();
        state
            .apply(&mutation(
                "entry",
                1,
                json!({"id":"root","type":"message","parentId":null,"timestamp":1,"lane":"main"}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "entry",
                2,
                json!({"id":"note","type":"custom","customType":"note","parentId":"root","timestamp":2,"lane":"main"}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "entry",
                3,
                json!({"id":"reply","type":"message","parentId":"note","timestamp":3,"lane":"main"}),
            ))
            .unwrap();

        let entries = state
            .find_entries(super::EntryQuery {
                entry_type: Some("message"),
                after_sequence: Some(3),
                ..Default::default()
            })
            .unwrap();
        let ids: Vec<_> = entries
            .iter()
            .map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
            .collect();
        assert_eq!(ids, vec![Some("root")]);

        let custom_entries = state
            .find_entries(super::EntryQuery {
                custom_type: Some("note"),
                oldest_first: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(custom_entries.len(), 1);
        assert_eq!(custom_entries[0].get("seq"), Some(&json!(2)));
    }

    #[test]
    fn applies_branch_bounds_in_the_requested_traversal_direction() {
        let mut state = SessionState::new();
        for (sequence, id, parent_id, entry_type) in [
            (1, "root", serde_json::Value::Null, "message"),
            (2, "middle", json!("root"), "custom"),
            (3, "leaf", json!("middle"), "message"),
        ] {
            let mut payload = json!({
                "id": id,
                "type": entry_type,
                "parentId": parent_id,
                "timestamp": sequence,
                "lane": "main",
            });
            if entry_type == "custom" {
                payload
                    .as_object_mut()
                    .unwrap()
                    .insert("customType".to_owned(), json!("note"));
            }
            state.apply(&mutation("entry", sequence, payload)).unwrap();
        }

        let newest_first = state
            .find_entries_on_branch_with_bounds(
                "leaf",
                super::EntryQuery::default(),
                super::BranchBounds {
                    stop_at_id: Some("middle"),
                    ..Default::default()
                },
            )
            .unwrap();
        let oldest_first = state
            .find_entries_on_branch_with_bounds(
                "leaf",
                super::EntryQuery {
                    oldest_first: true,
                    ..Default::default()
                },
                super::BranchBounds {
                    stop_at_id: Some("middle"),
                    ..Default::default()
                },
            )
            .unwrap();

        let newest_ids: Vec<_> = newest_first
            .iter()
            .map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
            .collect();
        let oldest_ids: Vec<_> = oldest_first
            .iter()
            .map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
            .collect();
        // 与 TypeScript `findEntriesOnBranch()` 一致：边界项被包含；oldest-first
        // 会先从 root 扫描，因此同一边界会保留其之前的祖先。
        assert_eq!(newest_ids, vec![Some("leaf"), Some("middle")]);
        assert_eq!(oldest_ids, vec![Some("root"), Some("middle")]);
    }

    #[test]
    fn traverses_branch_and_accumulates_usage_statistics() {
        let mut state = SessionState::new();
        state
            .apply(&mutation(
                "entry",
                1,
                json!({"id":"root","type":"message","parentId":null,"timestamp":1,"lane":"main"}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "entry",
                2,
                json!({"id":"reply","type":"message","parentId":"root","timestamp":2,"lane":"main"}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "record",
                3,
                json!({"id":"usage-1","type":"usage","lane":"main","timestamp":3,"usage":{"cacheRead":4,"input":5,"cacheWrite":6,"totalTokens":15,"cost":{"total":0.75}}}),
            ))
            .unwrap();

        let branch = state.find_entries_on_branch("reply", true, None).unwrap();
        let ids: Vec<_> = branch
            .iter()
            .map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
            .collect();
        assert_eq!(ids, vec![Some("root"), Some("reply")]);
        assert_eq!(
            state.stats(),
            super::SessionStats {
                message_count: 2,
                cached_tokens: 4,
                uncached_tokens: 11,
                total_tokens: 15,
                cost_total: 0.75,
            }
        );
    }

    #[test]
    fn accepts_negative_usage_corrections_without_partially_updating_statistics() {
        let mut state = SessionState::new();
        state
            .apply(&mutation(
                "record",
                1,
                json!({"id":"usage","type":"usage","lane":"main","timestamp":1,"usage":{"cacheRead":3,"input":10,"cacheWrite":2,"totalTokens":20,"cost":{"total":10.0}}}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "record",
                2,
                json!({"id":"correction","type":"usage","lane":"main","timestamp":2,"usage":{"cacheRead":0,"input":-2,"cacheWrite":0,"totalTokens":-2,"cost":{"total":-0.5}}}),
            ))
            .unwrap();
        assert_eq!(
            state.stats(),
            super::SessionStats {
                message_count: 0,
                cached_tokens: 3,
                uncached_tokens: 10,
                total_tokens: 18,
                cost_total: 9.5,
            }
        );

        let before_invalid_usage = state.stats();
        assert!(state
            .apply(&mutation(
                "record",
                3,
                json!({"id":"broken","type":"usage","lane":"main","timestamp":3,"usage":{"cacheRead":1,"input":1,"cacheWrite":1,"totalTokens":1,"cost":{}}}),
            ))
            .is_err());
        assert_eq!(state.stats(), before_invalid_usage);
        assert_eq!(state.next_sequence(), 3);
    }

    #[test]
    fn rejects_records_missing_their_required_run_reference() {
        let mut state = SessionState::new();
        for (sequence, record_type) in [(1, "step_attempt"), (1, "tool_started")] {
            let payload = json!({
                "id": "record",
                "type": record_type,
                "lane": "main",
                "timestamp": 1,
            });
            assert!(state.apply(&mutation("record", sequence, payload)).is_err());
            assert_eq!(state.next_sequence(), 1);
            assert!(state.find_records(Default::default()).unwrap().is_empty());
        }
    }

    #[test]
    fn queries_records_and_tracks_open_operations() {
        let mut state = SessionState::new();
        state
            .apply(&mutation(
                "record",
                1,
                json!({"id":"run-1","type":"operation_started","lane":"main","timestamp":1,"intent":{"kind":"run"}}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "record",
                2,
                json!({"id":"attempt-1","type":"step_attempt","lane":"main","timestamp":2,"runId":"run-1"}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "record",
                3,
                json!({"id":"finished-1","type":"operation_finished","lane":"main","timestamp":3,"runId":"run-1"}),
            ))
            .unwrap();
        state
            .apply(&mutation(
                "record",
                4,
                json!({"id":"run-2","type":"operation_started","lane":"main","timestamp":4,"intent":{"kind":"compaction"}}),
            ))
            .unwrap();

        let records = state
            .find_records(super::RecordQuery {
                run_id: Some("run-1"),
                oldest_first: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].get("seq"), Some(&json!(1)));
        assert_eq!(records[1].get("seq"), Some(&json!(2)));
        assert_eq!(records[2].get("seq"), Some(&json!(3)));

        let prompt_operations = state
            .find_records(super::RecordQuery {
                record_type: Some("operation_started"),
                operation_kind: Some("run"),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(prompt_operations.len(), 1);
        assert_eq!(prompt_operations[0].get("id"), Some(&json!("run-1")));

        // `operationKind` 不可脱离 `operation_started` 单独使用；该约束必须在
        // 空读取前生效，避免损坏调用被误判为“没有匹配记录”。
        let invalid_query = state.find_records(super::RecordQuery {
            operation_kind: Some("run"),
            ..Default::default()
        });
        assert_eq!(
            invalid_query.unwrap_err().code(),
            crate::SessionErrorCode::InvalidQuery
        );

        let open = state.find_open_operations("main", None).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].get("id"), Some(&json!("run-2")));

        state
            .apply(&mutation(
                "record",
                5,
                json!({"id":"finished-2","type":"operation_finished","lane":"main","timestamp":5,"runId":"run-2"}),
            ))
            .unwrap();
        let open = state.find_open_operations("main", None).unwrap();
        assert!(open.is_empty());
    }
}
