# Makima Egui 极简高级版布局方案

> 最终选型：`方案三：极简高级版`

## 1. 设计定位

这版不再追求“所有面板都能自由停靠”，而是追求一个更像成熟产品的工作台：

- 简洁
- 克制
- 高级
- 主次明确
- 默认稳定，不容易被拖乱

整体目标更接近 Codex 的克制感，但要适配 Makima 自己的 3D Unity 窗口。

---

## 2. 设计目标

目标不是“更自由”，而是“更安静、更聚焦、更产品化”。

核心目标：

1. 默认界面只突出一个主舞台，避免“面板感过强”。
2. 会话、聊天、Unity 角色、上下文信息形成稳定关系，不依赖用户自己拼布局。
3. 所有配置类、资源类、运维类能力尽量隐藏到侧边和底部。
4. 让界面看起来像完整产品，而不是开发工具的裸骨架。
5. 保留 dock 的能力，但只在很有限的层级使用。

---

## 3. 总体布局

建议采用“轻骨架 + 低噪音工作区”的布局。

```text
+--------------------------------------------------------------------------------------------------+
| Makima                                            Session Title                     Connected      |
+------+----------------------+-----------------------------------------------+--------------------+
|      |                      |                                               |                    |
| Act  | Conversations        | Main Workspace                                | Inspector          |
| Bar  |                      |                                               |                    |
|      | Search               | chat / avatar workspace                       | concise metadata   |
|      | session list         |                                               | only               |
|      |                      | large, calm, low-noise content area           |                    |
|      |                      |                                               | mode               |
|      |                      | focused content                               | model              |
|      |                      |                                               | task               |
|      |                      |                                               | voice / avatar     |
+------+----------------------+-----------------------------------------------+--------------------+
| Composer / Input Area                                                                    Status   |
+--------------------------------------------------------------------------------------------------+
```

### 关键原则

- 主舞台最大化，边栏最小化。
- 信息分层明显，但边框和标签暴露要尽量少。
- 用户第一眼应先看到内容，不是控制面板。
- 所有复杂能力默认隐藏在二级层级。

---

## 4. 产品结构

## 4.1 一级结构：主舞台

中心区域永远是主舞台，只承载最核心的工作内容：

- `Chat`
- `Avatar`

### Chat

- 保持 `Transcript + Composer` 一体化。
- 不拆成独立 tab。
- 时间线、工具状态、运行提示尽量作为聊天流内嵌区块出现。

### Avatar

- `Avatar` 是 Unity 主视窗，不是普通工具面板。
- 它应作为主舞台的一种形态出现。
- 建议支持两种展示方式：
  - `Chat Focus`：主舞台以聊天为主，Avatar 弱化或隐藏
  - `Avatar Focus`：主舞台中 Chat 和 Avatar 并列

### 主舞台原则

- 不做复杂 tab 条
- 不堆叠大量 panel chrome
- 减少“窗口工具感”
- 让内容本身成为视觉焦点

---

## 4.2 二级结构：左侧会话区

左侧保留，但尽量安静，承担会话导航职责。

左侧只重点承担：

- 会话搜索
- 会话列表
- 新建会话
- 少量会话操作

### 左侧风格要求

- 不要太多色块
- 不要每个条目都像按钮
- 更像内容导航列表，而不是功能菜单
- 会话卡片尽量扁平、轻边框、低对比

---

## 4.3 二级结构：右侧 Inspector

右侧只保留一个简洁版 `Inspector`。

它的存在原则是：

- 信息少而准
- 只展示“现在有用的上下文”
- 不做大型管理面板
- 不抢占主舞台存在感

### 建议保留的信息

- 当前 mode
- 当前 model
- 当前 task 状态
- token / cost
- Voice / Avatar 状态
- 当前焦点对象的元数据

### 建议移除的信息

- Memory 列表
- Knowledge 列表
- Persona 编辑
- Model 配置表单
- MCP 管理
- Diagnostics 详情页

---

## 4.4 三级结构：隐藏复杂度

复杂能力不应该在首页大面积暴露。

建议藏到：

- 左侧活动栏切换页
- 底部抽屉面板
- 局部弹层
- Inspector 的折叠区块

---

## 5. 视觉气质要求

这部分是给实现模型最重要的产品约束。

### 5.1 总体气质

- 偏 Codex 风格
- 极简
- 克制
- 安静
- 高级
- 不炫技

### 5.2 布局节奏

- 中间最大
- 左右更轻
- 顶部更薄
- 底部默认弱存在

### 5.3 面板质感

- 边框要少
- 大块分割线要轻
- 不要到处都是卡片和重描边
- 面板更像“层”，不是“盒子”

### 5.4 控件风格

- 按钮数量少
- 强按钮只保留一两个关键动作
- 次要操作通过 hover、右键、折叠或二级入口出现
- 不要把首页做成“控制台”

### 5.5 信息密度

- 会话列表中密度可以高
- 主舞台中密度要松
- Inspector 中信息要短
- Diagnostics / Audit 这类重信息面板延后暴露

## 6. Unity / Avatar 窗口要求

这是本项目和普通 Codex 风格最大的不同，建议单独约束。

## 6.1 Avatar 不是普通 dock tab

建议把 `Avatar` 定义成“主视窗类面板”，具备特殊规则：

- 默认不可关闭，只能切换显示权重
- 最小尺寸高于普通面板
- 不允许塞进底部区域
- 不允许进入左侧会话区
- 不允许缩成一个小工具卡片

## 6.2 需要稳定生命周期

如果未来使用 `wry + Unity WebGL` 或其他嵌入方案：

- 尽量避免频繁 mount/unmount
- 尽量避免频繁换父容器
- 尽量避免缩到极小尺寸

所以更适合：

- 作为固定 workspace slot
- 或作为主工作区中的特殊主 pane

## 6.3 Avatar 旁边需要哪些辅助信息

Avatar 工作时，右侧 Inspector 应优先显示：

- 语音连接状态
- 当前情绪/动作
- 嘴型驱动状态
- Unity FPS / render stats
- 最近一条角色指令

这样用户看 3D 窗口时，视线移动最短，同时不会被杂项配置干扰。

---

## 7. 对当前代码结构的落地建议

基于现有代码，建议从“全局 dock”改为“固定骨架 + 主舞台局部 dock”。

## 7.1 现在的主要问题

当前问题不是 dock 不能用，而是 dock 用得太满，导致产品感被稀释。

## 7.2 建议的新结构

### 外层固定骨架

由 `shell.rs` 控制：

- `top_bar`
- `activity_bar`
- `conversations_sidebar`
- `main_workspace`
- `inspector_sidebar`
- `bottom_drawer`
- `status_bar`

### 中间工作区再使用 dock

`dock.rs` 只负责 `main_workspace`，里面只保留：

- `Chat`
- `Avatar`

### 其他配置面板全部退出主 dock

这些内容不要再作为一级 `DockTab` 和主工作区平权：

- `Modes`
- `Persona`
- `ModelConfig`
- `Memory`
- `Knowledge`
- `Voice`
- `MCP`
- `Audit`
- `Diagnostics`

### 建议的新状态结构

建议新增结构：

```rust
enum SidebarSection {
    Sessions,
    Hidden,
}

enum BottomDrawerTab {
    TaskTimeline,
    Audit,
    Diagnostics,
    VoiceCall,
    McpActivity,
}

enum WorkspaceTab {
    Chat,
    Avatar,
}
```

如果一定保留资源与配置页，也建议通过左侧活动栏切页，而不是让它们直接进入主工作区。

---

## 8. 交互原则

## 8.1 首页只做一件事

- 默认进入就能开始对话
- 不让用户先理解布局系统
- 不让用户先管理面板

## 8.2 核心区域不允许被拖乱

建议默认不可关闭：

- `Chat`
- `Sessions`
- `Inspector`

`Avatar` 建议“可切换，不可误销毁”。

## 8.3 次要能力后置

- 配置类进入二级页
- 审计类进入底部抽屉
- 诊断类默认不曝光
- 资源类不抢主舞台

## 8.4 状态按需浮现

例如：

- 任务执行时出现轻量时间线提示
- 语音通话开始时底部出现 `Voice Call`
- MCP 异常时底部出现 `MCP Activity`

平时则尽量隐藏。

---

## 9. 推荐默认形态

## 9.1 Chat Focus

适用默认场景：

- 日常对话
- 内容创作
- 一般 agent 使用

布局：

- 左：会话区
- 中：Chat 主舞台
- 右：精简 Inspector
- 底部：默认弱化

## 9.2 Avatar Focus

适用场景：

- 语音陪伴
- 角色联动
- Unity 调试 / 展示

布局：

- 左：会话区
- 中：Chat + Avatar 双主舞台
- 右：Avatar 相关 Inspector
- 底部：Voice Call / Timeline 按需浮现

---

## 10. 给实现模型的最终指令

> 按“极简高级版”重构当前 `desktop-egui` 前端布局。  
> 不再把所有功能面板作为全局 `egui_dock` 的平级 tab。  
> 外层采用固定骨架：`Top Bar + Conversations Sidebar + Main Workspace + Inspector Sidebar + Composer + Bottom Drawer + Status Bar`。  
> `Main Workspace` 只保留 `Chat` 和 `Avatar(Unity)` 两种主工作面。  
> 左侧主要承担会话导航，资源、配置、集成类能力都弱化或后置，不要破坏首页极简感。  
> 右侧 `Inspector` 只展示简洁上下文信息，不承载重配置页面。  
> `Audit / Diagnostics / Voice Call / Task Timeline / MCP Activity` 收到 `Bottom Drawer`，默认隐藏，按状态浮现。  
> 视觉上减少边框、减少卡片、减少按钮、减少标签暴露，让中间主舞台最大化。  
> Unity Avatar 必须是主舞台级区域，不能作为普通小面板或工具 tab 对待。  
> 默认布局采用 `Chat Focus`，进入角色/语音场景后切换到 `Avatar Focus`。

---

## 11. 视觉规范版

这一节是给实现模型直接执行的，不讨论方向，只写死规则。

## 11.1 视觉关键词

- 极简
- 克制
- 高级
- 安静
- 专注
- 低噪音

禁止出现的气质：

- 面板堆砌感
- 重型控制台感
- 过多描边
- 花哨渐变
- 多色按钮泛滥
- “所有东西都很重要”的视觉混乱

---

## 11.2 布局尺寸规范

除特别说明外，统一使用 `8px` 网格。

### 顶部 Top Bar

- 高度：`56px`
- 左右内边距：`16px`
- 模块间距：`12px`
- 不允许超过两行
- 顶栏只承载：
  - 品牌 / 标题
  - 当前 session 标题
  - mode / model / connection 状态
  - `Chat` / `Avatar` 切换

### 左侧 Activity Bar

- 宽度：`52px`
- 图标区上下内边距：`12px`
- 图标按钮尺寸：`36px`
- 图标按钮圆角：`10px`
- 图标之间垂直间距：`8px`

### 左侧 Conversations Sidebar

- 默认宽度：`280px`
- 最小宽度：`240px`
- 最大宽度：`320px`
- 内边距：`12px`
- Search 区与列表区间距：`12px`
- Session item 高度：`40px` 到 `52px`

### 右侧 Inspector Sidebar

- 默认宽度：`300px`
- 最小宽度：`260px`
- 最大宽度：`340px`
- 内边距：`16px`
- 区块间距：`16px`
- 字段行高：`20px` 到 `24px`

### 主舞台 Main Workspace

- 占据剩余全部宽度
- 水平内边距：`20px`
- 垂直内边距：`16px`
- Chat Focus 下不允许被压缩得比 `640px` 更窄

### Composer 区

- 高度：`72px` 基准
- 多行输入上限高度：`160px`
- 左右内边距：`16px`
- 上下内边距：`12px`

### Bottom Drawer

- 收起高度：`0`
- 展开默认高度：`220px`
- 可扩展高度：`280px`
- 最大高度：窗口高度的 `32%`

---

## 11.3 圆角规范

统一减少圆角数量，不要全界面都是大圆角卡片。

- 顶级容器：`0px`
- 次级面板容器：`12px`
- 输入框：`12px`
- 普通按钮：`10px`
- 小按钮 / 图标按钮：`10px`
- 会话条目：`10px`
- 消息气泡：`16px`
- 状态标签：`999px` 胶囊

禁止：

- 一个页面里同时混用大量 `4 / 6 / 8 / 12 / 16 / 24` 无规律圆角
- 用超大圆角制造廉价“现代感”

---

## 11.4 间距规范

统一使用以下间距 token：

- `space-4 = 4px`
- `space-8 = 8px`
- `space-12 = 12px`
- `space-16 = 16px`
- `space-20 = 20px`
- `space-24 = 24px`
- `space-32 = 32px`

使用规则：

- 组件内部小元素：`4 / 8`
- 同一模块内元素：`8 / 12`
- 模块与模块之间：`16 / 20`
- 大区块切分：`24 / 32`

---

## 11.5 描边与分隔规范

整体遵循“弱边框、强留白”。

- 默认边框宽度：`1px`
- 分隔线颜色必须非常轻
- 非必要不要给每个子卡片都加边框
- 优先用留白区分，不优先用盒子区分

推荐规则：

- 顶栏底部允许一条 `1px` 弱分隔线
- 左右侧栏与主舞台之间允许一条弱分隔线
- 主舞台内部尽量不要出现大面积框线切割
- Inspector 内部区块可以无边框，仅靠标题和间距分层

---

## 11.6 阴影规范

- 默认无阴影
- 浮层 / 下拉 / 弹出层可使用轻阴影
- 不允许消息气泡、侧栏卡片、普通按钮到处带阴影

推荐阴影：

- 浮层阴影：`y=8 blur=24 alpha=0.16`
- 小菜单阴影：`y=4 blur=16 alpha=0.14`

---

## 11.7 字体层级规范

不要让字体层级过多，建议控制在 5 档以内。

### 字号

- 页面主标题：`20px`
- 区块标题：`15px`
- 正文：`13px`
- 次级正文：`12px`
- 辅助信息：`11px`

### 字重

- 主标题：`600`
- 区块标题：`600`
- 正文：`400`
- 标签 / 状态：`500`

### 行高

- 标题：`1.2`
- 正文：`1.45`
- 辅助信息：`1.4`

---

## 11.8 颜色使用规则

不在这里写死具体色值，但写死使用方式。

### 页面颜色层级

- `bg-app`：全局背景
- `bg-panel`：侧栏 / 顶栏 / 底部抽屉
- `bg-subtle`：输入区、轻状态块
- `bg-active`：选中态

### 文字颜色层级

- `text-primary`
- `text-secondary`
- `text-muted`
- `text-accent`

### 功能色

- `success`
- `warning`
- `error`
- `info`

### 使用限制

- 首页主界面同时可见的高饱和颜色不超过 `2` 种
- 红色或品牌色只用于关键焦点、选中态、主按钮
- 不允许把每个模块都做成不同颜色
- `Inspector` 尽量使用中性色，不要花

---

## 11.9 会话列表规范

会话区要像“内容导航”，不要像“按钮墙”。

### Search

- 高度：`36px`
- 圆角：`12px`
- 左侧可带小搜索图标
- 不要做重描边

### Session Item

- 高度：`44px`
- 左右内边距：`10px`
- 圆角：`10px`
- 默认背景透明或极浅
- Hover 才出现轻背景
- Active 才出现明确选中底色

### Session Item 内容

- 主标题单行截断
- 次级信息最多一行
- 未读状态仅用一个小点或细条提示
- 不要每项都塞很多 meta

---

## 11.10 Chat 主舞台规范

聊天区域是首页视觉中心。

### Transcript

- 宽内容、少边框
- 消息间距：`12px`
- 消息组之间间距：`16px`
- 顶部首屏留白要明显，不要太挤

### Message Bubble

- 最大阅读宽度：主舞台的 `78%`
- 圆角：`16px`
- 内边距：`12px 14px`
- 用户消息和助手消息只做轻度区分
- 不要把工具消息做得像另一套系统 UI

### Timeline / Tool State

- 优先嵌入聊天流顶部或消息间
- 使用轻背景块
- 默认折叠细节
- 不是独立大面板

---

## 11.11 Avatar 主舞台规范

Avatar 区必须有“主能力感”。

### Avatar Focus 布局

- Chat : Avatar 推荐宽度比 `5:5` 或 `5:6`
- 不允许 Avatar 比 `420px` 更窄
- 不允许 Avatar 缩成侧栏宽度

### Avatar 容器

- 圆角：`16px`
- 内边距：`0`
- 容器四周只允许轻边框或无边框
- 不要给 Unity 视窗外面套很多层卡片

### Avatar 周边信息

- 右侧 Inspector 只显示角色相关状态
- 不要把复杂控制按钮堆在 Avatar 画面上
- 画面上的悬浮控制最多保留：
  - mute
  - reconnect
  - fullscreen

---

## 11.12 Inspector 规范

Inspector 必须“短、准、轻”。

### 结构

- 区块标题使用小标题
- 每个区块 2 到 5 行信息即可
- 默认展示 4 到 6 个区块

### 内容优先级

- 第一优先：当前 mode / model / task
- 第二优先：voice / avatar 状态
- 第三优先：当前焦点消息元数据

### 禁止事项

- 不在 Inspector 里塞大型表单
- 不在 Inspector 里放长列表
- 不在 Inspector 里放复杂表格

---

## 11.13 Bottom Drawer 规范

Bottom Drawer 是“按需浮现的复杂度容器”。

### 默认规则

- 默认收起
- 仅在特定状态下自动展开
- 自动展开后不遮挡 Composer

### 承载内容

- `Task Timeline`
- `Voice Call`
- `Audit`
- `Diagnostics`
- `MCP Activity`

### 自动展开规则

- 有正在执行任务：优先显示 `Task Timeline`
- 开始语音通话：优先显示 `Voice Call`
- MCP 报错：优先显示 `MCP Activity`
- 诊断页不自动抢焦点，除非严重错误

### 关闭规则

- 用户手动关闭后，本轮任务内不重复强制弹出
- 严重错误可以仅用状态点提醒，不直接打断主舞台

---

## 11.14 面板显隐规则

这里把显隐逻辑直接写死。

### 默认进入应用

- 显示：Top Bar / Conversations Sidebar / Chat / Inspector / Composer / Status Bar
- 隐藏：Avatar / Bottom Drawer / Diagnostics / Audit / MCP Activity

### Chat Focus

- 主舞台：Chat
- Avatar：隐藏或弱化
- Inspector：显示
- Bottom Drawer：默认隐藏

### Avatar Focus

- 主舞台：Chat + Avatar
- Inspector：显示
- Bottom Drawer：可按需显示 `Voice Call` 或 `Task Timeline`

### 登录未完成

- 隐藏复杂布局
- 仅显示轻量登录界面
- 不展示底部复杂抽屉

### 窄屏模式

- 窗口宽度 `< 1200px`
- 右侧 Inspector 折叠为抽屉
- `Chat` 和 `Avatar` 改为切换，不做并排
- 左侧 Conversations Sidebar 可收起

---

## 11.15 动效规范

动效必须少，且服务于层级变化。

### 允许的动效

- Sidebar 显隐滑动：`160ms ~ 200ms`
- Bottom Drawer 展开收起：`180ms ~ 220ms`
- Hover 渐变：`120ms`
- 状态点颜色过渡：`120ms`

### 禁止的动效

- 弹簧感过强
- 频繁缩放
- 大量元素同时运动
- 首页持续呼吸闪烁

---

## 11.16 交互控件规范

### 主按钮

- 一个视图内最多 `1` 个主按钮
- 用于最关键动作，例如 `Send`

### 次按钮

- 用中性样式
- 不要抢主视觉

### 图标按钮

- 尺寸：`32px` 或 `36px`
- 圆角：`10px`
- 仅在 hover 时出现明显背景

### 切换按钮

- `Chat / Avatar` 切换必须像 workspace switch，不像普通 tab
- 推荐胶囊分段按钮或轻分段控制

---

## 11.17 实现约束摘要

给实现模型执行时，必须遵守以下硬规则：

1. `Main Workspace` 只允许 `Chat` 和 `Avatar(Unity)` 成为主舞台。
2. 不允许把 `Memory / Knowledge / Modes / Persona / Model / MCP / Voice / Diagnostics / Audit` 作为首页平级主面板铺开。
3. `Inspector` 只展示短信息，不允许承担重配置。
4. `Bottom Drawer` 默认隐藏，只按状态浮现。
5. 整体优先用留白分层，不优先用盒子和重边框分层。
6. 视觉焦点必须始终落在中间主舞台，而不是左侧功能区。
7. Unity Avatar 不能被实现成普通工具 tab 或小窗卡片。

---

## 12. 一句话结论

这版方案的核心不是“dock 得更自由”，而是“把复杂度藏起来，让 `Chat` 和 `Avatar(Unity)` 成为唯一主角”，做出更像成熟产品的 Makima 桌面前端。
