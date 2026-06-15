use std::collections::HashMap;
use tonic::{Request, Response, Status};

use crate::proto::{
    shell_service_server::ShellService,
    file_service_server::FileService,
    http_service_server::HttpService,
    document_service_server::DocumentService,
    sandbox_service_server::SandboxService,
    *,
};
use crate::tools::{ShellExecutor, FileOperations, HttpClient};
use crate::document::TextChunker;
use crate::sandbox::{PathSecurity, CommandFilter, NetworkPolicy};

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
