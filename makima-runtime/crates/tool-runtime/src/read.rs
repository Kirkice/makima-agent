//! TypeScript `read` 工具的 Rust 生产实现。

use std::{fs, path::PathBuf};

use protocol::{TextOrImageContent, ToolCall, ToolDefinition};
use sandbox::{FileAccess, PolicySandbox, Sandbox, SandboxDecision, SandboxPolicy};
use serde_json::{json, Value};

use crate::{Tool, ToolExecutionContext, ToolExecutionError, ToolOutput};

const MAX_LINES: usize = 2_000;
const MAX_BYTES: usize = 50 * 1024;

/// 只允许读取 Session 工作目录的内置文件工具。
pub struct ReadTool {
    workspace_root: PathBuf,
    sandbox: PolicySandbox,
}

impl ReadTool {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Result<Self, ToolExecutionError> {
        let workspace_root = absolute_workspace_root(workspace_root.into())?;
        let policy = SandboxPolicy::workspace_only(workspace_root.clone())
            .map_err(|error| ToolExecutionError::new(error.to_string()))?;
        Ok(Self {
            workspace_root,
            sandbox: PolicySandbox::new(policy),
        })
    }

    fn resolve_path(&self, path: &str) -> Result<PathBuf, ToolExecutionError> {
        let normalized = path.trim_start_matches('@').replace('\u{00a0}', " ");
        let requested = PathBuf::from(normalized);
        let absolute = if requested.is_absolute() {
            requested
        } else {
            self.workspace_root.join(requested)
        };
        match self.sandbox.check_file_access(&absolute, FileAccess::Read) {
            SandboxDecision::Allow => {}
            SandboxDecision::Deny(reason) => {
                return Err(ToolExecutionError::new(reason.to_string()));
            }
        }

        // 对已存在目标执行真实路径校验，阻止位于 workspace 内的符号链接逃逸到外部。
        let canonical = absolute.canonicalize().map_err(|error| {
            ToolExecutionError::new(format!("Failed to read {}: {error}", absolute.display()))
        })?;
        let canonical_root = self.workspace_root.canonicalize().map_err(|error| {
            ToolExecutionError::new(format!(
                "Failed to resolve workspace {}: {error}",
                self.workspace_root.display()
            ))
        })?;
        if !canonical.starts_with(&canonical_root) {
            return Err(ToolExecutionError::new("路径不在 Sandbox 允许的根目录内"));
        }
        Ok(canonical)
    }
}

impl Tool for ReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read".to_owned(),
            description: format!(
                "Read the contents of a file. Supports text files and images (jpg, png, gif, webp, bmp). Images are sent as attachments. For text files, output is truncated to {MAX_LINES} lines or {}KB (whichever is hit first). Use offset/limit for large files. When you need the full file, continue with offset until complete.",
                MAX_BYTES / 1024
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file to read (relative or absolute)" },
                    "offset": { "type": "number", "description": "Line number to start reading from (1-indexed)" },
                    "limit": { "type": "number", "description": "Maximum number of lines to read" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            execution_mode: protocol::ToolExecutionMode::Parallel,
        }
    }

    fn execute(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
    ) -> Result<ToolOutput, ToolExecutionError> {
        context.check_active()?;
        let path = required_string(&call.input, "path")?;
        let offset = optional_positive_integer(&call.input, "offset")?;
        let limit = optional_positive_integer(&call.input, "limit")?;
        let absolute = self.resolve_path(path)?;
        let bytes = fs::read(&absolute).map_err(|error| {
            ToolExecutionError::new(format!("Failed to read {}: {error}", absolute.display()))
        })?;
        context.check_active()?;

        if let Some(mime_type) = image_mime_type(&bytes) {
            return image_output(bytes, mime_type);
        }
        read_text(bytes, path, offset, limit)
    }
}

fn absolute_workspace_root(path: PathBuf) -> Result<PathBuf, ToolExecutionError> {
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|error| ToolExecutionError::new(error.to_string()))?
            .join(path)
    };
    Ok(absolute)
}

fn required_string<'a>(input: &'a Value, name: &str) -> Result<&'a str, ToolExecutionError> {
    input.get(name).and_then(Value::as_str).ok_or_else(|| {
        ToolExecutionError::new(format!("Tool input property has invalid type: {name}"))
    })
}

fn optional_positive_integer(
    input: &Value,
    name: &str,
) -> Result<Option<usize>, ToolExecutionError> {
    let Some(value) = input.get(name) else {
        return Ok(None);
    };
    let number = value
        .as_f64()
        .filter(|number| number.is_finite() && number.fract() == 0.0 && *number > 0.0)
        .ok_or_else(|| ToolExecutionError::new(format!("{name} must be a positive integer")))?;
    if number > usize::MAX as f64 {
        return Err(ToolExecutionError::new(format!("{name} is too large")));
    }
    Ok(Some(number as usize))
}

fn read_text(
    bytes: Vec<u8>,
    display_path: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<ToolOutput, ToolExecutionError> {
    let text = String::from_utf8_lossy(&bytes);
    let all_lines = text.split('\n').collect::<Vec<_>>();
    let total_file_lines = all_lines.len();
    let start = offset.unwrap_or(1) - 1;
    if start >= total_file_lines {
        return Err(ToolExecutionError::new(format!(
            "Offset {} is beyond end of file ({total_file_lines} lines total)",
            offset.unwrap_or(1)
        )));
    }
    let selected_end = limit
        .map(|limit| start.saturating_add(limit).min(total_file_lines))
        .unwrap_or(total_file_lines);
    // 必须先按 TS 相同方式重建选区，再做截断。这样文件末尾的换行会保留在输出中，
    // 但不会被截断统计误算成额外一行。
    let selected_content = all_lines[start..selected_end].join("\n");
    let truncation = truncate_head(&selected_content);
    let start_display = start + 1;
    let mut output = if truncation.first_line_exceeds_limit {
        format!(
            "[Line {start_display} is {}, exceeds 50.0KB limit. Use bash: sed -n '{start_display}p' {display_path} | head -c {MAX_BYTES}]",
            format_size(all_lines[start].len())
        )
    } else {
        truncation.content.clone()
    };
    if let Some(kind) = truncation.truncated_by {
        let end_display = start_display + truncation.output_lines.saturating_sub(1);
        let suffix = if kind == "bytes" {
            " (50.0KB limit)"
        } else {
            ""
        };
        output.push_str(&format!(
            "\n\n[Showing lines {start_display}-{end_display} of {total_file_lines}{suffix}. Use offset={} to continue.]",
            end_display + 1
        ));
    } else if limit.is_some() && selected_end < total_file_lines {
        output.push_str(&format!(
            "\n\n[{} more lines in file. Use offset={} to continue.]",
            total_file_lines - selected_end,
            selected_end + 1
        ));
    }
    Ok(ToolOutput {
        content: vec![TextOrImageContent::Text { text: output }],
        details: truncation.truncated_by.map(|_| truncation.details()),
    })
}

/// 与 TypeScript `truncateHead()` 对齐的内部结果。字节数始终按 UTF-8 计算，且只输出
/// 完整行；尾随换行属于内容字节，但不属于额外行。
struct Truncation {
    content: String,
    truncated_by: Option<&'static str>,
    total_lines: usize,
    total_bytes: usize,
    output_lines: usize,
    output_bytes: usize,
    first_line_exceeds_limit: bool,
}

impl Truncation {
    fn details(&self) -> Value {
        json!({
            "truncation": {
                "content": self.content,
                "truncated": self.truncated_by.is_some(),
                "truncatedBy": self.truncated_by,
                "totalLines": self.total_lines,
                "totalBytes": self.total_bytes,
                "outputLines": self.output_lines,
                "outputBytes": self.output_bytes,
                "lastLinePartial": false,
                "firstLineExceedsLimit": self.first_line_exceeds_limit,
                "maxLines": MAX_LINES,
                "maxBytes": MAX_BYTES
            }
        })
    }
}

fn truncate_head(content: &str) -> Truncation {
    let total_bytes = content.len();
    let counted_content = content.strip_suffix('\n').unwrap_or(content);
    let lines = if counted_content.is_empty() {
        Vec::new()
    } else {
        counted_content.split('\n').collect::<Vec<_>>()
    };
    let total_lines = lines.len();
    if total_lines <= MAX_LINES && total_bytes <= MAX_BYTES {
        return Truncation {
            content: content.to_owned(),
            truncated_by: None,
            total_lines,
            total_bytes,
            output_lines: total_lines,
            output_bytes: total_bytes,
            first_line_exceeds_limit: false,
        };
    }
    if lines.first().is_some_and(|line| line.len() > MAX_BYTES) {
        return Truncation {
            content: String::new(),
            truncated_by: Some("bytes"),
            total_lines,
            total_bytes,
            output_lines: 0,
            output_bytes: 0,
            first_line_exceeds_limit: true,
        };
    }

    let mut output = Vec::new();
    let mut output_bytes = 0;
    let mut truncated_by = "lines";
    for line in lines.iter().take(MAX_LINES) {
        let line_bytes = line.len() + usize::from(!output.is_empty());
        if output_bytes + line_bytes > MAX_BYTES {
            truncated_by = "bytes";
            break;
        }
        output.push(*line);
        output_bytes += line_bytes;
    }
    let content = output.join("\n");
    Truncation {
        output_bytes: content.len(),
        content,
        truncated_by: Some(truncated_by),
        total_lines,
        total_bytes,
        output_lines: output.len(),
        first_line_exceeds_limit: false,
    }
}

fn image_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

fn image_output(bytes: Vec<u8>, mime_type: &'static str) -> Result<ToolOutput, ToolExecutionError> {
    if mime_type == "image/bmp" {
        return Ok(ToolOutput::text(
            "Read image file [image/bmp]\n[Image omitted: configure an imageProcessor to convert BMP images.]",
        ));
    }
    Ok(ToolOutput {
        content: vec![
            TextOrImageContent::Text {
                text: format!("Read image file [{mime_type}]"),
            },
            TextOrImageContent::Image {
                data: encode_base64(&bytes),
                mime_type: mime_type.to_owned(),
            },
        ],
        details: None,
    })
}

fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use protocol::{TextOrImageContent, ToolCall};
    use serde_json::json;

    use crate::{Tool, ToolExecutionContext, ToolOutput};

    use super::ReadTool;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "makima-read-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn context() -> ToolExecutionContext {
        let (updates, _receiver) = std::sync::mpsc::channel();
        ToolExecutionContext::new(Default::default(), None, updates, "1".to_owned())
    }

    fn execute_text(tool: &ReadTool, input: serde_json::Value) -> ToolOutput {
        tool.execute(
            &ToolCall {
                tool_call_id: "1".into(),
                tool_name: "read".into(),
                input,
            },
            &context(),
        )
        .unwrap()
    }

    fn text_content(output: &ToolOutput) -> &str {
        let TextOrImageContent::Text { text } = &output.content[0] else {
            panic!("expected text")
        };
        text
    }

    #[test]
    fn reads_offsets_limits_and_default_line_truncation() {
        let root = temp_dir("text");
        fs::write(
            root.join("large.txt"),
            (1..=2500)
                .map(|line| format!("line {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        let tool = ReadTool::new(&root).unwrap();
        let limited = execute_text(&tool, json!({"path":"large.txt","offset":41,"limit":20}));
        assert!(text_content(&limited).starts_with("line 41\nline 42"));
        assert!(text_content(&limited)
            .ends_with("[2440 more lines in file. Use offset=61 to continue.]"));

        let truncated = execute_text(&tool, json!({"path":"large.txt"}));
        assert!(text_content(&truncated)
            .ends_with("[Showing lines 1-2000 of 2500. Use offset=2001 to continue.]"));
        let details = &truncated.details.as_ref().unwrap()["truncation"];
        assert_eq!(details["truncatedBy"], "lines");
        assert_eq!(details["outputLines"], 2000);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trailing_newline_at_line_limit_is_not_truncated() {
        let root = temp_dir("trailing-newline");
        let content = format!("{}\n", vec!["x"; 2000].join("\n"));
        fs::write(root.join("exact.txt"), &content).unwrap();
        let output = execute_text(&ReadTool::new(&root).unwrap(), json!({"path":"exact.txt"}));
        assert_eq!(text_content(&output), content);
        assert!(output.details.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_byte_truncation_with_utf8_metadata() {
        let root = temp_dir("utf8");
        // 行数低于 2000，但 UTF-8 字节数超过 50 KiB，确保先命中字节门禁。
        fs::write(
            root.join("utf8.txt"),
            vec!["中文内容测试中文内容测试中文内容测试"; 1_000].join("\n"),
        )
        .unwrap();
        let output = execute_text(&ReadTool::new(&root).unwrap(), json!({"path":"utf8.txt"}));
        let details = &output.details.as_ref().unwrap()["truncation"];
        assert_eq!(details["truncatedBy"], "bytes");
        assert!(details["outputBytes"].as_u64().unwrap() <= 50 * 1024);
        assert!(details["totalBytes"].as_u64().unwrap() > 50 * 1024);
        assert!(text_content(&output).contains("(50.0KB limit)"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_offsets_beyond_end_of_file() {
        let root = temp_dir("offset");
        fs::write(root.join("short.txt"), "one\ntwo\nthree").unwrap();
        let error = ReadTool::new(&root)
            .unwrap()
            .execute(
                &ToolCall {
                    tool_call_id: "1".into(),
                    tool_name: "read".into(),
                    input: json!({"path":"short.txt","offset":100}),
                },
                &context(),
            )
            .unwrap_err();
        assert_eq!(
            error.message(),
            "Offset 100 is beyond end of file (3 lines total)"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_png_by_content_and_returns_base64_attachment() {
        let root = temp_dir("image");
        fs::write(root.join("image.bin"), b"\x89PNG\r\n\x1a\nabc").unwrap();
        let tool = ReadTool::new(&root).unwrap();
        let output = tool
            .execute(
                &ToolCall {
                    tool_call_id: "1".into(),
                    tool_name: "read".into(),
                    input: json!({"path":"image.bin"}),
                },
                &context(),
            )
            .unwrap();
        assert!(
            matches!(&output.content[1], TextOrImageContent::Image { mime_type, .. } if mime_type == "image/png")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn omits_bmp_without_an_image_processor() {
        let root = temp_dir("bmp");
        fs::write(root.join("image.bin"), b"BMfixture").unwrap();
        let output = execute_text(&ReadTool::new(&root).unwrap(), json!({"path":"image.bin"}));
        assert_eq!(
            text_content(&output),
            "Read image file [image/bmp]\n[Image omitted: configure an imageProcessor to convert BMP images.]"
        );
        assert_eq!(output.content.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_links_that_escape_the_workspace() {
        use std::os::unix::fs::symlink;

        let root = temp_dir("symlink-root");
        let outside = temp_dir("symlink-outside");
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        symlink(outside.join("secret.txt"), root.join("secret.txt")).unwrap();
        let error = ReadTool::new(&root)
            .unwrap()
            .execute(
                &ToolCall {
                    tool_call_id: "1".into(),
                    tool_name: "read".into(),
                    input: json!({"path":"secret.txt"}),
                },
                &ToolExecutionContext::new(Default::default(), None),
            )
            .unwrap_err();
        assert_eq!(error.message(), "路径不在 Sandbox 允许的根目录内");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
