use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::proto::{
    shell_service_server::ShellService,
    file_service_server::FileService,
    http_service_server::HttpService,
    document_service_server::DocumentService,
    sandbox_service_server::SandboxService,
    checkpoint_service_server::CheckpointService,
    file_tracker_service_server::FileTrackerService,
    token_counter_service_server::TokenCounterService,
    *,
};
use crate::tools::{ShellExecutor, FileOperations, HttpClient};
use crate::document::TextChunker;
use crate::sandbox::{PathSecurity, CommandFilter, NetworkPolicy};
use crate::checkpoint::CheckpointManager;
use crate::tracker::FileTrackerManager;
use crate::tokens::TokenCounter;

// Shell Service
pub struct ShellServiceImpl { executor: ShellExecutor }

impl ShellServiceImpl {
    pub fn new() -> Self { Self { executor: ShellExecutor::new() } }
}

#[tonic::async_trait]
impl ShellService for ShellServiceImpl {
    async fn execute(&self, request: Request<ShellRequest>) -> Result<Response<ShellResponse>, Status> {
        let req = request.into_inner();
        match self.executor.execute(&req.command, &req.working_dir, req.timeout_seconds as u64).await {
            Ok((stdout, stderr, exit_code)) => Ok(Response::new(ShellResponse {
                success: exit_code == 0, stdout, stderr, exit_code, blocked: false, block_reason: String::new(),
            })),
            Err(e) => {
                let blocked = matches!(e, crate::tools::shell::ShellError::Blocked(_));
                Ok(Response::new(ShellResponse {
                    success: false, stdout: String::new(), stderr: e.to_string(), exit_code: -1, blocked, block_reason: e.to_string(),
                }))
            }
        }
    }
}

// File Service
pub struct FileServiceImpl { operations: FileOperations }

impl FileServiceImpl {
    pub fn new() -> Self { Self { operations: FileOperations::new() } }
}

#[tonic::async_trait]
impl FileService for FileServiceImpl {
    async fn read_file(&self, request: Request<ReadFileRequest>) -> Result<Response<ReadFileResponse>, Status> {
        let req = request.into_inner();
        match self.operations.read_file(&req.path, &req.base_dir).await {
            Ok(content) => Ok(Response::new(ReadFileResponse { success: true, content, error: String::new() })),
            Err(e) => Ok(Response::new(ReadFileResponse { success: false, content: String::new(), error: e.to_string() })),
        }
    }

    async fn write_file(&self, request: Request<WriteFileRequest>) -> Result<Response<WriteFileResponse>, Status> {
        let req = request.into_inner();
        match self.operations.write_file(&req.path, &req.content, &req.base_dir).await {
            Ok(bytes) => Ok(Response::new(WriteFileResponse { success: true, bytes_written: bytes, error: String::new() })),
            Err(e) => Ok(Response::new(WriteFileResponse { success: false, bytes_written: 0, error: e.to_string() })),
        }
    }

    async fn list_directory(&self, request: Request<ListDirRequest>) -> Result<Response<ListDirResponse>, Status> {
        let req = request.into_inner();
        match self.operations.list_directory(&req.path, &req.base_dir).await {
            Ok(entries) => Ok(Response::new(ListDirResponse {
                success: true,
                entries: entries.into_iter().map(|e| DirEntry { name: e.name, is_dir: e.is_dir, size: e.size }).collect(),
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(ListDirResponse { success: false, entries: vec![], error: e.to_string() })),
        }
    }
}

// HTTP Service
pub struct HttpServiceImpl { client: HttpClient }

impl HttpServiceImpl {
    pub fn new() -> Self { Self { client: HttpClient::new() } }
}

#[tonic::async_trait]
impl HttpService for HttpServiceImpl {
    async fn request(&self, request: Request<HttpRequest>) -> Result<Response<HttpResponse>, Status> {
        let req = request.into_inner();
        let headers: HashMap<String, String> = req.headers.into_iter().collect();
        let body = if req.body.is_empty() { None } else { Some(req.body) };
        match self.client.request(&req.url, &req.method, headers, body, req.timeout_seconds as u64).await {
            Ok((status_code, body)) => Ok(Response::new(HttpResponse {
                success: true, status_code: status_code as u32, body, blocked: false, block_reason: String::new(),
            })),
            Err(e) => {
                let blocked = matches!(e, crate::tools::http::HttpError::Blocked(_));
                Ok(Response::new(HttpResponse {
                    success: false, status_code: 0, body: String::new(), blocked, block_reason: e.to_string(),
                }))
            }
        }
    }
}

// Document Service
pub struct DocumentServiceImpl;
impl DocumentServiceImpl { pub fn new() -> Self { Self } }

#[tonic::async_trait]
impl DocumentService for DocumentServiceImpl {
    async fn chunk_text(&self, request: Request<ChunkTextRequest>) -> Result<Response<ChunkTextResponse>, Status> {
        let req = request.into_inner();
        let chunker = TextChunker::new(req.chunk_size as usize, req.overlap as usize);
        let chunks = chunker.chunk_text(&req.text);
        let proto_chunks: Vec<TextChunk> = chunks.into_iter().map(|c| TextChunk { index: c.index, content: c.content, token_count: c.token_count }).collect();
        let total = proto_chunks.len() as u32;
        Ok(Response::new(ChunkTextResponse { chunks: proto_chunks, total_chunks: total }))
    }

    async fn estimate_tokens(&self, request: Request<EstimateTokensRequest>) -> Result<Response<EstimateTokensResponse>, Status> {
        let req = request.into_inner();
        Ok(Response::new(EstimateTokensResponse { token_count: (req.text.len() / 4) as u32 }))
    }
}

// Sandbox Service
pub struct SandboxServiceImpl;
impl SandboxServiceImpl { pub fn new() -> Self { Self } }

#[tonic::async_trait]
impl SandboxService for SandboxServiceImpl {
    async fn check_path(&self, request: Request<PathCheckRequest>) -> Result<Response<PathCheckResponse>, Status> {
        let req = request.into_inner();
        let security = PathSecurity::new(&req.base_dir);
        match security.validate_path(&req.path) {
            Ok(resolved) => Ok(Response::new(PathCheckResponse { allowed: true, resolved_path: resolved.display().to_string(), reason: String::new() })),
            Err(e) => Ok(Response::new(PathCheckResponse { allowed: false, resolved_path: String::new(), reason: e.to_string() })),
        }
    }

    async fn check_command(&self, request: Request<CommandCheckRequest>) -> Result<Response<CommandCheckResponse>, Status> {
        let req = request.into_inner();
        let filter = CommandFilter::new();
        match filter.validate_command(&req.command) {
            Ok(()) => Ok(Response::new(CommandCheckResponse { allowed: true, matched_pattern: String::new() })),
            Err(e) => Ok(Response::new(CommandCheckResponse { allowed: false, matched_pattern: e.to_string() })),
        }
    }

    async fn check_url(&self, request: Request<UrlCheckRequest>) -> Result<Response<UrlCheckResponse>, Status> {
        let req = request.into_inner();
        let policy = NetworkPolicy::new();
        match policy.validate_url(&req.url) {
            Ok(()) => Ok(Response::new(UrlCheckResponse { allowed: true, reason: String::new() })),
            Err(e) => Ok(Response::new(UrlCheckResponse { allowed: false, reason: e.to_string() })),
        }
    }
}

// Checkpoint Service
pub struct CheckpointServiceImpl {
    manager: Arc<CheckpointManager>,
}

impl CheckpointServiceImpl {
    pub fn new(manager: Arc<CheckpointManager>) -> Self {
        Self { manager }
    }
}

#[tonic::async_trait]
impl CheckpointService for CheckpointServiceImpl {
    async fn save(&self, request: Request<SaveCheckpointRequest>) -> Result<Response<SaveCheckpointResponse>, Status> {
        let req = request.into_inner();
        let base_dir = Path::new(&req.base_dir);
        let dir = Path::new(&req.directory);

        match self.manager.save(dir, base_dir, &req.label).await {
            Ok(checkpoint) => Ok(Response::new(SaveCheckpointResponse {
                success: true,
                checkpoint: Some(CheckpointInfo {
                    checkpoint_id: checkpoint.id,
                    label: checkpoint.label,
                    created_at: checkpoint.created_at,
                    file_count: checkpoint.file_count(),
                    total_bytes: checkpoint.total_bytes(),
                }),
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(SaveCheckpointResponse {
                success: false,
                checkpoint: None,
                error: e,
            })),
        }
    }

    async fn restore(&self, request: Request<RestoreCheckpointRequest>) -> Result<Response<RestoreCheckpointResponse>, Status> {
        let req = request.into_inner();
        let base_dir = Path::new(&req.base_dir);

        match self.manager.restore(&req.checkpoint_id, base_dir).await {
            Ok(restored_files) => Ok(Response::new(RestoreCheckpointResponse {
                success: true,
                restored_files,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(RestoreCheckpointResponse {
                success: false,
                restored_files: 0,
                error: e,
            })),
        }
    }

    async fn list(&self, request: Request<ListCheckpointsRequest>) -> Result<Response<ListCheckpointsResponse>, Status> {
        let req = request.into_inner();
        let base_dir = Path::new(&req.base_dir);
        let checkpoints = self.manager.list(base_dir);

        let infos: Vec<CheckpointInfo> = checkpoints
            .into_iter()
            .map(|c| CheckpointInfo {
                checkpoint_id: c.id,
                label: c.label,
                created_at: c.created_at,
                file_count: c.file_count(),
                total_bytes: c.total_bytes(),
            })
            .collect();

        Ok(Response::new(ListCheckpointsResponse {
            success: true,
            checkpoints: infos,
            error: String::new(),
        }))
    }

    async fn delete(&self, request: Request<DeleteCheckpointRequest>) -> Result<Response<DeleteCheckpointResponse>, Status> {
        let req = request.into_inner();
        let base_dir = Path::new(&req.base_dir);

        match self.manager.delete(&req.checkpoint_id, base_dir) {
            Ok(()) => Ok(Response::new(DeleteCheckpointResponse {
                success: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(DeleteCheckpointResponse {
                success: false,
                error: e,
            })),
        }
    }
}

// File Tracker Service
pub struct FileTrackerServiceImpl {
    manager: Arc<FileTrackerManager>,
}

impl FileTrackerServiceImpl {
    pub fn new(manager: Arc<FileTrackerManager>) -> Self {
        Self { manager }
    }
}

#[tonic::async_trait]
impl FileTrackerService for FileTrackerServiceImpl {
    async fn snapshot(&self, request: Request<SnapshotRequest>) -> Result<Response<SnapshotResponse>, Status> {
        let req = request.into_inner();
        let base_dir = Path::new(&req.base_dir);

        match self.manager.snapshot(&req.path, base_dir).await {
            Ok(tracked) => Ok(Response::new(SnapshotResponse {
                success: true,
                file_hash: Some(FileHash {
                    path: req.path,
                    sha256: tracked.sha256,
                    size: tracked.size,
                    modified_at: tracked.modified_at,
                }),
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(SnapshotResponse {
                success: false,
                file_hash: None,
                error: e,
            })),
        }
    }

    async fn check_diff(&self, request: Request<CheckDiffRequest>) -> Result<Response<CheckDiffResponse>, Status> {
        let req = request.into_inner();
        let base_dir = Path::new(&req.base_dir);

        match self.manager.check_diff(&req.path, base_dir).await {
            Ok((exists, modified, deleted, current_sha256, tracked_sha256)) => {
                Ok(Response::new(CheckDiffResponse {
                    exists,
                    modified,
                    deleted,
                    current_sha256,
                    tracked_sha256,
                    error: String::new(),
                }))
            }
            Err(e) => Ok(Response::new(CheckDiffResponse {
                exists: false,
                modified: false,
                deleted: false,
                current_sha256: String::new(),
                tracked_sha256: String::new(),
                error: e,
            })),
        }
    }

    async fn get_history(&self, request: Request<GetHistoryRequest>) -> Result<Response<GetHistoryResponse>, Status> {
        let req = request.into_inner();
        let base_dir = Path::new(&req.base_dir);

        match self.manager.get_history(&req.path, base_dir) {
            Ok(history) => {
                let entries: Vec<FileHistoryEntry> = history
                    .into_iter()
                    .map(|h| FileHistoryEntry {
                        action: h.action,
                        timestamp: h.timestamp,
                        sha256: h.sha256,
                    })
                    .collect();
                Ok(Response::new(GetHistoryResponse {
                    success: true,
                    history: entries,
                    error: String::new(),
                }))
            }
            Err(e) => Ok(Response::new(GetHistoryResponse {
                success: false,
                history: vec![],
                error: e,
            })),
        }
    }

    async fn clear_history(&self, request: Request<ClearHistoryRequest>) -> Result<Response<ClearHistoryResponse>, Status> {
        let req = request.into_inner();
        let base_dir = Path::new(&req.base_dir);

        match self.manager.clear_history(&req.path, base_dir) {
            Ok(()) => Ok(Response::new(ClearHistoryResponse {
                success: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(ClearHistoryResponse {
                success: false,
                error: e,
            })),
        }
    }
}

// Token Counter Service
pub struct TokenCounterServiceImpl {
    counter: Arc<TokenCounter>,
}

impl TokenCounterServiceImpl {
    pub fn new(counter: Arc<TokenCounter>) -> Self {
        Self { counter }
    }
}

#[tonic::async_trait]
impl TokenCounterService for TokenCounterServiceImpl {
    async fn count(&self, request: Request<CountTokensRequest>) -> Result<Response<CountTokensResponse>, Status> {
        let req = request.into_inner();
        let model = if req.model.is_empty() { "gpt-4" } else { &req.model };

        match self.counter.count(&req.text, model) {
            Ok(token_count) => Ok(Response::new(CountTokensResponse {
                success: true,
                token_count,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(CountTokensResponse {
                success: false,
                token_count: 0,
                error: e,
            })),
        }
    }

    async fn truncate(&self, request: Request<TruncateTokensRequest>) -> Result<Response<TruncateTokensResponse>, Status> {
        let req = request.into_inner();
        let model = if req.model.is_empty() { "gpt-4" } else { &req.model };

        match self.counter.truncate(&req.text, req.max_tokens, model, req.preserve_start) {
            Ok((truncated_text, original_tokens, truncated_tokens, was_truncated)) => {
                Ok(Response::new(TruncateTokensResponse {
                    success: true,
                    truncated_text,
                    original_tokens,
                    truncated_tokens,
                    was_truncated,
                    error: String::new(),
                }))
            }
            Err(e) => Ok(Response::new(TruncateTokensResponse {
                success: false,
                truncated_text: String::new(),
                original_tokens: 0,
                truncated_tokens: 0,
                was_truncated: false,
                error: e,
            })),
        }
    }

    async fn batch_count(&self, request: Request<BatchCountRequest>) -> Result<Response<BatchCountResponse>, Status> {
        let req = request.into_inner();
        let model = if req.model.is_empty() { "gpt-4" } else { &req.model };

        match self.counter.batch_count(&req.texts, model) {
            Ok((token_counts, total_tokens)) => Ok(Response::new(BatchCountResponse {
                success: true,
                token_counts,
                total_tokens,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(BatchCountResponse {
                success: false,
                token_counts: vec![],
                total_tokens: 0,
                error: e,
            })),
        }
    }
}
