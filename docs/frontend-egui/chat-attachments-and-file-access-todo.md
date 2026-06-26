# Chat 附件与文件访问改造清单

本文用于指导后续实现，让聊天中的“附件”真正能被后端和模型使用，并对“工作区外文件访问”提供合理、可控的导入方案。

当前结论：

- 前端聊天框里选择的附件，只保存在桌面端本地状态里，没有随消息发送到后端。
- 模型工具链只能读取当前工作区内的相对路径，工作区外的绝对路径会被路径安全检查拦截。

相关代码位置：

- 前端附件 UI 状态：[apps/desktop-egui/src/ui/chat/composer.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/chat/composer.rs:309)
- 前端发送消息逻辑：[apps/desktop-egui/src/app.rs](/H:/Project/makima-agent/apps/desktop-egui/src/app.rs:211)
- Python 文件工具安全限制：[apps/backend/src/makima/tools/file_tool.py](/H:/Project/makima-agent/apps/backend/src/makima/tools/file_tool.py:14)
- Rust 路径安全限制：[services/tool-runtime/src/sandbox/path_security.rs](/H:/Project/makima-agent/services/tool-runtime/src/sandbox/path_security.rs:22)

## 一、现状问题

### 1. 附件没有进入消息发送链路

- `composer` 中选中文件后，只存进 `state.chat.composer.attachments`
- `exec_send_message()` 发送时只传了：
  - `text`
  - `mode_slug`
  - `model_override`
- 没有任何附件上传、附件元数据传输、附件内容注入上下文的逻辑

需要完成：

- 设计并实现“附件随消息提交”的数据流
- 明确后端如何接收附件
- 明确模型最终如何看到附件内容

### 2. 工作区外绝对路径会被安全机制拦截

- 当前文件工具只允许访问 `tool_working_dir` 下的内容
- 外部绝对路径会被判定为 path traversal
- 这是安全设计，不应该简单放开

需要完成：

- 不要让模型直接读取任意绝对路径
- 提供“导入到工作区”或“上传到受控目录”的机制

## 二、推荐实现目标

建议把目标分成两步：

### 目标 A：让聊天附件真正可用

用户在聊天框里附加文本类文件后，发送消息时：

- 前端自动上传附件到后端
- 后端保存到受控临时目录
- 后端把文本内容读出并加入任务上下文
- 模型在首轮就能看到附件内容，而不是只看到文件名

### 目标 B：让工作区外文件可被“导入”，而不是直接越权读取

用户选择工作区外文件时：

- 前端或后端先将文件复制到受控目录
- 再把该受控路径交给后端/工具链
- 模型只处理导入后的路径

## 三、具体任务拆解

### 任务 1：补一个后端附件上传接口

新增接口建议：

- `POST /api/tasks/attachments/upload`

建议请求形式：

- `multipart/form-data`
- 字段至少包含：
  - 文件本体
  - `session_id` 或临时会话标识

建议返回内容：

- `attachment_id`
- `original_name`
- `stored_path`
- `mime_type`
- `size`
- `is_text`

实现要求：

- 文件必须保存到受控目录，例如：
  - `.makima/uploads/...`
  - 或会话级临时目录
- 文件名需要做安全处理，避免覆盖和注入
- 要限制上传大小
- 要记录 MIME type 或至少扩展名

建议落点：

- 新增 `apps/backend/src/makima/routes/attachments.py`
- 在 `app.py` 中注册 router

### 任务 2：前端发送消息时，把附件一起提交

当前 `exec_send_message()` 没处理附件，需要补上：

实现要求：

- 发送前先遍历 `state.chat.composer.attachments`
- 对每个附件调用上传接口
- 上传成功后拿到 `attachment_id` / `stored_path`
- 再把这些附件信息作为任务请求的一部分发送给后端

建议做法：

- 在 `apps/desktop-egui/src/api/tasks.rs` 里扩展任务请求结构
- 增加 `attachments` 字段
- 附件元素建议包含：
  - `attachment_id`
  - `name`
  - `mime_type`
  - `stored_path`
  - `is_text`

前端 UI 还应补充：

- 上传中状态
- 上传失败状态
- 单个附件失败提示

### 任务 3：后端任务入口支持 attachments 字段

当前任务流只接收文本，需要扩展：

- 修改任务创建/流式任务入口的请求 schema
- 增加 `attachments` 字段

需要完成：

- 后端收到消息时，能拿到附件列表
- 附件信息进入任务上下文构建逻辑

建议检查位置：

- `apps/backend/src/makima/routes/tasks.py`
- `packages/schemas/src/makima_schemas/api.py`
- 与任务流相关的 schema / request model

### 任务 4：文本附件自动注入上下文

这是最重要的一步，否则即使上传了也只是“有个文件”。

建议策略：

- 对文本类文件自动读取内容
- 限制单文件最大注入长度
- 长文件截断，并在上下文中说明已截断

建议支持的文本类：

- `.txt`
- `.md`
- `.json`
- `.yaml`
- `.yml`
- `.py`
- `.rs`
- `.js`
- `.ts`
- `.tsx`
- `.jsx`
- `.html`
- `.css`
- `.cs`
- `.java`
- `.cpp`
- `.h`
- `.toml`
- `.xml`
- `.csv`

建议上下文格式：

```text
Attached files:

[1] ShadowController.cs
Path: attachments/session_xxx/ShadowController.cs
Content:
...file content...
```

实现要求：

- 只对文本文件自动读内容
- 二进制文件不直接塞给模型
- 内容需要有大小限制，例如：
  - 单文件最多 N KB
  - 总附件上下文最多 N KB

### 任务 5：非文本附件先做保底处理

图片、PDF、二进制目前不一定要一次性做好深度解析，但至少要有保底方案。

建议第一版：

- 图片：只传文件名、大小、MIME type
- PDF：先不自动全文解析，只传元信息
- 二进制：只传元信息，不注入正文

返回给模型的上下文可以类似：

```text
[2] diagram.png
Type: image/png
Size: 182 KB
Note: Binary attachment uploaded but not inlined into context.
```

后续可扩展：

- 图片 OCR
- PDF 文本抽取
- Office 文档解析

### 任务 6：为“工作区外文件”增加导入机制

不要让模型直接读取：

- `G:\Project\...`
- `C:\Users\...`

而是做成“导入后使用”。

推荐方案：

- 用户在桌面端选择任意本地文件
- 前端上传到后端受控目录
- 后端返回受控路径
- 后续任务只使用这个受控路径

这样做的好处：

- 不需要放宽 path traversal 安全限制
- 文件来源清晰
- 更适合审计和清理

明确要求：

- 不要直接放宽 `file_tool.py` 和 `path_security.rs` 的工作区限制
- 这两层安全边界应该保留

### 任务 7：让工具链知道“附件已经存在”

如果后续 agent 需要主动再读附件文件，而不是只看首轮注入文本，需要：

- 把上传后的附件保存到 agent 可访问的工作目录下
- 或在任务上下文里声明附件相对路径

建议：

- 附件最终落点放在工具工作目录之内的某个子目录
- 例如：
  - `.makima/runtime_attachments/...`
  - 或当前会话工作目录下 `attachments/...`

这样 `read_file` 工具就能合法读取，不会触发 path traversal

### 任务 8：补清理策略

上传附件后需要有生命周期管理。

需要完成：

- 临时附件目录的清理策略
- 会话结束是否清理
- 定期清理过期文件

建议第一版：

- 保留最近 N 天
- 启动时清理旧文件
- 或按 session 目录分组管理

### 任务 9：补错误提示与用户反馈

当前用户体验的问题之一是：

- UI 看起来像“已经附加了文件”
- 实际模型完全没拿到

需要补的反馈：

- 上传中：显示 `Uploading`
- 上传成功：显示 `Uploaded`
- 上传失败：显示具体失败原因
- 发送时如果附件没成功上传，应阻止发送或明确提示

建议改动位置：

- [apps/desktop-egui/src/ui/chat/composer.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/chat/composer.rs:84)
- `AttachmentStatus` 相关状态流转

### 任务 10：补测试

至少需要覆盖这些场景：

- 文本文件附件上传成功
- 文本文件内容被注入上下文
- 非文本附件只注入元信息
- 工作区外绝对路径不会直接传给文件工具
- 上传后生成的受控路径可被读取
- 超大文件被拒绝或截断
- 删除附件后不会继续发送

建议测试层：

- 后端 API 测试
- 前端最少做状态流转检查
- 若任务 schema 改动较大，要补端到端测试

## 四、实现顺序建议

建议按这个顺序推进：

1. 增加附件上传接口
2. 前端发送消息前先上传附件
3. 扩展任务请求 schema，带上 attachments
4. 后端把文本附件注入上下文
5. 非文本附件先做元信息保底
6. 补 UI 状态反馈
7. 补清理策略和测试

## 五、明确不要做的事

- 不要简单移除 path traversal 检查
- 不要让模型直接读取任意盘符绝对路径
- 不要只在前端显示附件而不接入任务请求
- 不要把超大文件无上限直接塞进 prompt

## 六、第一版验收标准

满足以下条件即可认为第一版可用：

1. 用户附加 `.cs` / `.py` / `.txt` / `.md` 文件后，发送消息时模型能看到文件内容。
2. 用户选择工作区外文件时，不再报 path traversal，而是先上传/导入到受控目录。
3. 非文本文件至少能显示“已上传，但未内联内容”的说明。
4. 前端能正确显示附件上传成功/失败状态。
5. 工具层的工作区安全边界仍然保留。
