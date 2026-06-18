# Makima Agent Egui Frontend Implementation Path

## Goal

Build a Rust desktop frontend with `egui` that feels close to Codex-class agent tooling, but is shaped around Makima's current backend and future agent features.

This document focuses on:

- what the frontend should include
- what can be built immediately with current APIs
- what backend gaps exist today
- how to phase implementation so we get value early instead of waiting for a "perfect" UI

---

## Product Direction

The frontend should not be just "a prettier CLI". It should become an agent control surface:

- chat-first
- mode-aware
- streaming-first
- observability-friendly
- voice-capable
- configuration-heavy but safe

The closest mental model is:

- left side: conversation and workspace navigation
- center: streaming chat/task execution surface
- right side: context, controls, telemetry, tools, model and voice settings

---

## Recommended Tech Stack

### Core

- `eframe` for native desktop shell
- `egui` for UI
- `tokio` for async runtime
- `reqwest` for REST + SSE transport
- `serde` / `serde_json` for API models
- `anyhow` / `thiserror` for error handling
- `tracing` for app logs
- `directories` for local app config/cache paths

### Helpful UI crates

- `egui_extras` for tables
- `egui_dock` for dockable panels
- `egui_plot` for token/cost/latency graphs
- `egui-toast` or custom toast system for notifications
- `rfd` for file picker
- `copypasta` for clipboard

### Optional later

- `rodio` or `cpal` for local audio playback/recording
- `notify` for local config file watching
- `taffy` not needed unless we later hybridize layout logic

---

## UI Information Architecture

## Main Shell

Use a 3-column layout:

- left rail
  - recent chats
  - pinned chats
  - saved searches
  - quick actions
- center content
  - active conversation
  - streaming messages
  - task/tool timeline
  - input composer
- right inspector
  - mode
  - model config
  - token/cost estimate
  - context/memory/knowledge hits
  - voice and runtime controls

Top bar:

- session title
- current server/environment
- current mode badge
- model badge
- online/offline health
- command palette entry

Bottom status bar:

- auth state
- SSE connected/disconnected
- task state
- token estimate
- current voice

---

## Feature Set

## 1. Chat Workspace

Must have:

- recent conversations list
- create / rename / delete conversation
- chat bubbles with role styling
- streaming assistant output
- tool call cards
- tool result cards
- task error rendering
- retry last input
- copy message
- collapse long tool traces

Nice to have:

- branch conversation
- diff between two assistant answers
- search within current chat
- message bookmarks
- export markdown / JSON

Current backend support:

- `POST /sessions`, `GET /sessions`, `PATCH /sessions/{id}`, `DELETE /sessions/{id}` in [apps/backend/src/makima/routes/sessions.py](/f:/Project/AIProject/makima-agent/apps/backend/src/makima/routes/sessions.py:17)
- `POST /tasks` SSE stream and `GET /tasks/{task_id}` in [apps/backend/src/makima/routes/tasks.py](/f:/Project/AIProject/makima-agent/apps/backend/src/makima/routes/tasks.py:21)

Backend gap:

- there is no dedicated "list messages for a session" endpoint yet
- frontend phase 1 can reconstruct visible chat from task stream + local cache
- but for a real app, add `GET /sessions/{id}/messages`

---

## 2. Recent Chats / Navigation

Must have:

- sort by updated time
- unread/active marker
- draft indicator
- search by title
- favorite/pin

Nice to have:

- tags
- folders or workspaces
- "today / this week / older" grouping

---

## 3. Chat Composer

Must have:

- multiline input
- submit / stop generation
- drag-drop file placeholder
- slash command support
- mode switch shortcut
- estimated token/cost preview before send

Nice to have:

- prompt snippets
- reusable prompt templates
- quick insert from memory / knowledge
- local markdown preview toggle

---

## 4. Task Timeline

This is one of the biggest places to feel "more advanced than chat".

Show a live execution strip for:

- thinking phase
- memory recall
- retrieval
- tool dispatch
- tool result
- completion
- error

Current backend support:

- SSE event types already exist in [apps/backend/src/makima/routes/tasks.py](/f:/Project/AIProject/makima-agent/apps/backend/src/makima/routes/tasks.py:45)

Recommended visualization:

- compact timeline above messages
- expandable per-step drawer
- inline tool cards inside transcript
- right-panel raw event inspector for debugging

---

## 5. Cost / Token / Runtime Estimation

Must have:

- estimated input tokens before send
- rolling per-session token total
- estimated session cost
- current task elapsed time

Nice to have:

- by-model cost table
- daily/weekly local usage dashboard
- latency histogram

Current backend support:

- there is token counting support in tool runtime, but no dedicated frontend-friendly API yet

Backend gap:

- add `/api/usage/estimate`
- or expose token estimation through existing runtime service

Frontend phase 1 fallback:

- local approximation based on character count and selected model

---

## 6. Mode Switching

Must have:

- mode dropdown
- mode detail panel
- mode description / when-to-use
- built-in vs custom mode marker
- reload project modes

Nice to have:

- compare modes
- clone mode
- inline edit draft

Current backend support:

- `GET /api/modes`, `GET /api/modes/{slug}`, `POST /api/modes`, `DELETE /api/modes/{slug}`, `POST /api/modes/reload` in [apps/backend/src/makima/routes/modes.py](/f:/Project/AIProject/makima-agent/apps/backend/src/makima/routes/modes.py:12)

Backend gap:

- no session-bound mode selection API is visible in current routes
- README mentions one, but current code does not expose it

Recommended backend addition:

- `PATCH /sessions/{id}/mode`
- include active mode in session detail

---

## 7. Model Configuration

Must have:

- current provider / base URL / model
- temperature
- max steps
- timeout
- active env source indicator

Nice to have:

- per-mode model override editor
- model presets
- test connection
- model capability tags: tools / long context / low latency / cheap

Current backend reality:

- app reads config and supports mode-level model fields, but there is no full dedicated model config API yet

Backend gap:

- add `/api/settings/model`
- add `/api/settings/runtime`
- add a safe "masked secrets" response shape

Important rule:

- never echo raw API keys back to the UI
- show `configured: true/false`, last updated timestamp, and provider name instead

---

## 8. MCP Management

This should be first-class, not hidden in settings.

Must have:

- list configured MCP servers
- enabled/disabled status
- transport type
- startup health
- last error
- manual reconnect

Nice to have:

- add/remove server
- inspect advertised tools/resources
- permission scopes
- per-mode MCP enablement

Current backend reality:

- there is no visible MCP management API in current Makima backend routes

Backend gap:

- add `/api/mcp/servers`
- add `/api/mcp/servers/{id}/health`
- add `/api/mcp/servers/{id}/tools`
- add enable/disable and restart actions

Frontend phase 1 fallback:

- local-only configuration screen backed by a frontend config file

---

## 9. Voice Management

Split this into two separate layers:

- voice conversation runtime
- TTS voice selection / cloned voice management

Must have:

- TTS provider selector
- active voice ID
- voice test button
- mic input device selector
- speaker output selector
- push-to-talk / always-listen toggle
- input/output volume meters

Nice to have:

- VAD threshold tuning
- STT/TTS provider fallback chain
- saved voice presets
- live transcript overlay

Current project reality:

- CLI TTS exists in [cli.py](/f:/Project/AIProject/makima-agent/cli.py:206)
- voice runtime exists in [services/voice-runtime/agent.py](/f:/Project/AIProject/makima-agent/services/voice-runtime/agent.py:43)

Backend gap:

- add a voice settings API if frontend should manage provider/voice centrally

Recommended frontend tabs:

- `Voice Chat`
- `TTS Voices`
- `Audio Devices`
- `Speech Diagnostics`

---

## 10. Persona Management

Must have:

- show current persona
- reload persona
- view default persona
- edit in-memory persona draft

Nice to have:

- diff current vs default
- "strictness", "warmth", "verbosity" sliders mapped onto persona templates

Current backend support:

- persona endpoints exist in [apps/backend/src/makima/routes/persona.py](/f:/Project/AIProject/makima-agent/apps/backend/src/makima/routes/persona.py:14)

Important warning:

- current update route is in-memory only unless `.makima/persona.yaml` is also updated
- UI should show this clearly

---

## 11. Memory Panel

Must have:

- memory service status
- list memories
- search memories
- delete memory

Nice to have:

- pin memory
- mark as wrong / stale
- memory categories

Current backend support:

- memory endpoints exist in [apps/backend/src/makima/routes/memory.py](/f:/Project/AIProject/makima-agent/apps/backend/src/makima/routes/memory.py:17)

---

## 12. Knowledge / RAG Panel

Must have:

- upload document
- list documents
- delete document
- test retrieval

Nice to have:

- chunk preview
- source snippet preview
- ingestion progress
- retrieval debug mode

Current backend support:

- knowledge routes are available in [apps/backend/src/makima/routes/knowledge.py](/f:/Project/AIProject/makima-agent/apps/backend/src/makima/routes/knowledge.py:22)

---

## 13. Audit / Admin Panel

Must have:

- audit log table
- filters by severity/action/resource
- request detail drawer

Nice to have:

- export CSV
- suspicious activity highlighting

Current backend support:

- admin-only audit route in [apps/backend/src/makima/routes/audit.py](/f:/Project/AIProject/makima-agent/apps/backend/src/makima/routes/audit.py:18)

---

## 14. Health / Diagnostics

This is easy to skip and always regretted later.

Must have:

- backend health
- auth health
- SSE stream state
- current API base URL
- local config path
- error log console

Nice to have:

- latency probes
- retry diagnostics
- dependency matrix: LLM / memory / voice / MCP / storage

---

## 15. Settings / Secrets UX

Must have:

- local-only settings file
- masked secret inputs
- "configured / missing" badges instead of raw secret display
- import/export non-secret settings

Recommended storage split:

- frontend UI state: local app config
- real secrets: OS keyring if possible, fallback to local ignored file

Recommended crates:

- `keyring` for OS credential store

Important rule:

- do not store third-party API keys in plain UI state if avoidable

---

## Recommended Desktop App Structure

```text
apps/desktop-egui/
  Cargo.toml
  src/
    main.rs
    app.rs
    bootstrap.rs
    theme.rs
    routes.rs
    commands.rs
    config/
      mod.rs
      app_config.rs
      secure_store.rs
    api/
      mod.rs
      auth.rs
      sessions.rs
      tasks.rs
      modes.rs
      persona.rs
      memory.rs
      knowledge.rs
      audit.rs
      mcp.rs
      voice.rs
    state/
      mod.rs
      app_state.rs
      chat_state.rs
      task_state.rs
      settings_state.rs
    ui/
      shell.rs
      top_bar.rs
      side_nav.rs
      chat/
        transcript.rs
        bubbles.rs
        composer.rs
        timeline.rs
      panels/
        inspector.rs
        modes.rs
        model_config.rs
        memory.rs
        knowledge.rs
        voice.rs
        mcp.rs
        audit.rs
        diagnostics.rs
```

---

## UX Style Guidance

If we want "similar to Codex or better", do not make this look like a default debug dashboard.

Direction:

- dark graphite base
- restrained red accents for Makima identity
- high information density
- clear hierarchy
- no oversized padding everywhere
- keyboard-first interactions

Important interaction patterns:

- command palette
- global quick switcher
- dockable right panels
- resizable chat/tool split
- persistent session sidebar
- rich hover states for tool steps and context hits

---

## Implementation Phases

## Phase 0: Foundation

Goal:

- create desktop app shell
- add login
- add health check
- add config/secrets storage

Deliverables:

- app boots
- user can configure server URL
- user can log in
- app stores token securely

---

## Phase 1: Usable Chat Client

Goal:

- make it genuinely better than CLI for daily chat use

Deliverables:

- recent sessions
- create/rename/delete session
- SSE task streaming
- chat transcript
- tool call/result rendering
- stop/retry actions
- basic token estimate

Backend dependency:

- add message history endpoint if we want durable transcript reload

---

## Phase 2: Agent Control Surface

Goal:

- expose mode, persona, memory, knowledge, diagnostics

Deliverables:

- mode switch panel
- persona panel
- memory panel
- knowledge upload/retrieve panel
- diagnostics panel

---

## Phase 3: Advanced Runtime Controls

Goal:

- bring in model settings, voice settings, and MCP management

Deliverables:

- model config UI
- voice config UI
- MCP list/health UI
- local cost dashboard

Backend dependency:

- add settings APIs
- add MCP APIs
- add voice settings APIs

---

## Phase 4: Power User Experience

Goal:

- make the app feel premium and agent-native

Deliverables:

- command palette
- chat search
- bookmarks
- branch conversation
- timeline inspector
- audit/admin tools
- local usage analytics

---

## Phase 5: "Better Than Codex" Layer

This is where Makima can differentiate.

Ideas:

- memory confidence inspector
- prompt/context composition viewer
- retrieval provenance panel
- live agent reasoning phase map without exposing unsafe chain-of-thought
- mode recommendation engine
- voice persona presets
- session snapshots and replay
- one-click "convert chat to knowledge"

---

## Backend Gaps To Prioritize

These will unblock the frontend fastest:

1. `GET /sessions/{id}/messages`
2. `PATCH /sessions/{id}/mode`
3. masked settings endpoints for model/runtime/voice
4. MCP management endpoints
5. token/cost estimate endpoint
6. unified health/dependencies endpoint

---

## Suggested Development Order

1. Build desktop shell and auth.
2. Implement sessions + chat + SSE streaming.
3. Add message persistence endpoint in backend.
4. Add mode/persona/memory/knowledge inspector panels.
5. Add settings and secure secret storage.
6. Add voice and MCP management.
7. Add analytics, command palette, and premium UX touches.

---

## Practical First Milestone

If we want the fastest believable demo, target this:

- login
- session list
- streaming chat
- tool timeline
- mode dropdown
- right inspector with token estimate + persona + memory status

That is enough to look like a real agent workbench instead of a toy.

---

## Recommendation

Start with a native desktop app under `apps/desktop-egui`, not a web wrapper. `egui` is strongest when we embrace:

- fast internal tooling UI
- dense control surfaces
- dockable inspector patterns
- debug + operations visibility

If you want, the next step I can do is continue from this README and give you:

1. a concrete `apps/desktop-egui` crate scaffold plan
2. the Rust module skeleton
3. the first-screen wireframe mapped to `egui` panels
4. the backend API contract list the frontend should consume
