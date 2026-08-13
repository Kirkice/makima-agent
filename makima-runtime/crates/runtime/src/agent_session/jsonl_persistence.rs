//! JSONL v4 Session Store 的 AgentSession 适配器。
//!
//! AgentSession 只提交业务事件，本模块负责转换成 Store 所需的 entry 结构。转换
//! 规则集中在这里，避免 JSONL 字段名扩散到领域状态机，也便于未来迁移到其他后端。

use serde_json::{Map, Value, json};
use session::{JsonlSessionStore, NewEntry};

use super::{PersistenceEvent, SessionPersistence, SessionPersistenceError};

/// 使用 `main` lane 保存 AgentSession 的稳定历史。
const MAIN_LANE: &str = "main";

/// 将现有 JSONL Store 作为 AgentSession 的持久化端口。
pub struct JsonlSessionPersistence {
    store: JsonlSessionStore,
    next_entry_sequence: u64,
}

impl JsonlSessionPersistence {
    /// 接管一个已打开的 Store。
    ///
    /// entry ID 仅需在 Store 内唯一，因此从当前 mutation 数量之后开始分配即可；
    /// Store 仍是唯一的 parent、timestamp 与全局 sequence 赋值者。
    pub fn new(store: JsonlSessionStore) -> Self {
        Self {
            next_entry_sequence: store.mutations().len() as u64,
            store,
        }
    }

    /// 返回底层 Store 的只读引用，供仓库级查询和诊断使用。
    pub fn store(&self) -> &JsonlSessionStore {
        &self.store
    }

    /// 取回底层 Store 的所有权，供关闭 lease 或上层切换 Session 时使用。
    pub fn into_store(self) -> JsonlSessionStore {
        self.store
    }

    fn next_entry_id(&mut self, kind: &str) -> String {
        self.next_entry_sequence += 1;
        format!("agent-session-{kind}-{}", self.next_entry_sequence)
    }

    fn append_entry(
        &mut self,
        kind: &str,
        mut fields: Map<String, Value>,
    ) -> Result<(), SessionPersistenceError> {
        fields.insert("id".to_owned(), Value::String(self.next_entry_id(kind)));
        fields.insert("type".to_owned(), Value::String(kind.to_owned()));
        self.store
            .append_entry(NewEntry {
                lane: MAIN_LANE.to_owned(),
                fields,
            })
            .map(|_| ())
            .map_err(|error| SessionPersistenceError::new(error.to_string()))
    }
}

impl SessionPersistence for JsonlSessionPersistence {
    fn persist(&mut self, event: PersistenceEvent) -> Result<(), SessionPersistenceError> {
        match event {
            PersistenceEvent::TranscriptItemFinished(item) => {
                let message = serde_json::to_value(item)
                    .map_err(|error| SessionPersistenceError::new(error.to_string()))?;
                self.append_entry(
                    "message",
                    Map::from_iter([(String::from("message"), message)]),
                )
            }
            PersistenceEvent::ModelChanged(model) => self.append_entry(
                "model_change",
                Map::from_iter([
                    (String::from("provider"), Value::String(model.provider)),
                    (String::from("modelId"), Value::String(model.id)),
                ]),
            ),
            PersistenceEvent::ThinkingLevelChanged(level) => self.append_entry(
                "thinking_level_change",
                Map::from_iter([(String::from("thinkingLevel"), json!(level))]),
            ),
        }
    }
}
