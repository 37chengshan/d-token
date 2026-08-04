//! Minimal d-token-owned Model Context Protocol transport.
//!
//! The transport is intentionally stdio-only and does not persist MCP request bodies.  Tool
//! errors are stable, redacted codes so malformed inputs cannot leak prompts or credentials.

use crate::AppState;
use dtoken_context_store::{
    parse_memory_write_request, MemoryAccessScope, MemoryCapabilityPermission,
    ResolvedMemoryCapability,
};
use dtoken_types::{CompressionPolicy, OpticalEndpoint, Protocol};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{HashSet, VecDeque},
    io,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

pub const MCP_IMPLEMENTATION_REVISION: &str = "dtoken-mcp-stdio-v1";
pub const MAX_MCP_COMPRESS_BODY_BYTES: usize = 1024 * 1024;
pub const MAX_MCP_MESSAGE_BYTES: usize = MAX_MCP_COMPRESS_BODY_BYTES + 16 * 1024;
const MAX_CANCELLED_REQUESTS: usize = 128;
const STDIO_JSONRPC_TRANSPORT: &str = "stdio-json-rpc";

const PROTOCOL_VERSION: &str = "2024-11-05";
const TOOL_COMPRESS: &str = "headroom_compress";
const TOOL_RETRIEVE: &str = "headroom_retrieve";
const TOOL_DELETE: &str = "headroom_delete";
const TOOL_STATS: &str = "headroom_stats";
const TOOL_MEMORY_LIST: &str = "headroom_memory_list";
const TOOL_MEMORY_EXPORT: &str = "headroom_memory_export";
const TOOL_MEMORY_DELETE: &str = "headroom_memory_delete";
const TOOL_MEMORY_WRITE: &str = "headroom_memory_write";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolError {
    code: &'static str,
    message: &'static str,
}

impl McpToolError {
    pub(crate) fn invalid_arguments(_detail: &'static str) -> Self {
        Self {
            code: "invalid_arguments",
            message: "Tool arguments are invalid.",
        }
    }

    pub(crate) fn request_too_large() -> Self {
        Self {
            code: "request_too_large",
            message: "The compression body exceeds the d-token MCP limit.",
        }
    }

    pub(crate) fn route_context_unavailable() -> Self {
        Self {
            code: "route_context_unavailable",
            message: "The requested d-token route is unavailable for MCP compression.",
        }
    }

    pub(crate) fn route_context_mismatch() -> Self {
        Self {
            code: "route_context_mismatch",
            message: "The MCP route reference does not match the active d-token route.",
        }
    }

    pub(crate) fn route_policy_mismatch() -> Self {
        Self {
            code: "route_policy_mismatch",
            message: "The requested compression policy does not match the active d-token route.",
        }
    }

    pub(crate) fn memory_unavailable() -> Self {
        Self {
            code: "memory_authorization_unavailable",
            message: "SharedContext requires a managed d-token MCP capability.",
        }
    }

    pub(crate) fn memory_scope_denied() -> Self {
        Self {
            code: "memory_access_denied",
            message: "The managed SharedContext scope cannot perform this operation.",
        }
    }

    fn content(&self) -> Value {
        json!({
            "isError": true,
            "content": [{
                "type": "text",
                "text": serde_json::to_string(&json!({
                    "code": self.code,
                    "message": self.message,
                })).expect("static MCP tool errors serialize"),
            }]
        })
    }
}

#[derive(Clone)]
pub struct McpServer {
    state: AppState,
    retrieve_capability: Option<String>,
    memory_capability: Option<ManagedMemoryCapability>,
    cancelled_request_ids: Arc<Mutex<CancellationBookkeeping>>,
}

/// This grant is injected by a d-token-owned managed MCP transaction, not by
/// an MCP tool caller.  It pins the capability to one project/Agent/session
/// boundary and is intentionally never serialized into a result or log.
#[derive(Debug, Clone)]
pub(crate) struct ManagedMemoryCapability {
    resolved: ResolvedMemoryCapability,
}

impl ManagedMemoryCapability {
    fn scope(&self) -> MemoryAccessScope {
        self.resolved.scope.clone()
    }

    fn permits(&self, permission: MemoryCapabilityPermission) -> bool {
        self.resolved.permits(permission)
    }
}

#[derive(Default)]
struct CancellationBookkeeping {
    ids: HashSet<String>,
    arrival_order: VecDeque<String>,
}

impl McpServer {
    pub fn new(state: AppState) -> Self {
        Self::new_with_capabilities(state, None, None)
    }

    /// The capability is supplied by the managed MCP launcher at process
    /// start. It is intentionally memory-only and must never be logged or
    /// returned in a tool result.
    pub fn new_with_retrieve_capability(
        state: AppState,
        retrieve_capability: Option<String>,
    ) -> Self {
        Self::new_with_capabilities(state, retrieve_capability, None)
    }

    /// Builds the same scoped-memory server used by the managed launcher.
    ///
    /// This is intentionally hidden from generated documentation: it exists so
    /// cross-surface contract tests can prove authorization and scope behavior
    /// without reading `DTOKEN_MCP_MEMORY_CAPABILITY` from the process
    /// private capability document. The capability remains process-local and
    /// is never returned through MCP.
    #[doc(hidden)]
    pub fn new_with_managed_memory_capability_for_test(
        state: AppState,
        capability: ResolvedMemoryCapability,
    ) -> Self {
        Self::new_with_capabilities(
            state,
            None,
            Some(ManagedMemoryCapability {
                resolved: capability,
            }),
        )
    }

    pub fn new_with_resolved_memory_capability(
        state: AppState,
        capability: ResolvedMemoryCapability,
    ) -> Self {
        Self::new_with_capabilities(
            state,
            None,
            Some(ManagedMemoryCapability {
                resolved: capability,
            }),
        )
    }

    pub(crate) fn new_with_capabilities(
        state: AppState,
        retrieve_capability: Option<String>,
        memory_capability: Option<ManagedMemoryCapability>,
    ) -> Self {
        Self {
            state,
            retrieve_capability: retrieve_capability.filter(|value| valid_capability(value)),
            memory_capability,
            cancelled_request_ids: Arc::new(Mutex::new(CancellationBookkeeping::default())),
        }
    }

    pub fn handlers_available() -> bool {
        Self::transport_name().is_some()
    }

    pub fn transport_name() -> Option<&'static str> {
        Some(STDIO_JSONRPC_TRANSPORT)
    }

    pub fn available_tool_names(&self) -> Vec<&'static str> {
        let mut tools = vec![TOOL_COMPRESS, TOOL_STATS];
        if self.retrieve_capability.is_some() {
            tools.push(TOOL_RETRIEVE);
            tools.push(TOOL_DELETE);
        }
        if self.memory_capability.is_some() {
            tools.push(TOOL_MEMORY_LIST);
            tools.push(TOOL_MEMORY_EXPORT);
            tools.push(TOOL_MEMORY_DELETE);
            tools.push(TOOL_MEMORY_WRITE);
        }
        tools
    }

    pub fn handle_jsonrpc(&self, request: Value) -> Option<Value> {
        let Some(object) = request.as_object() else {
            return Some(jsonrpc_error(Value::Null, -32600, "Invalid Request"));
        };
        let id = object.get("id").cloned();
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || object.get("method").and_then(Value::as_str).is_none()
            || id.as_ref().is_some_and(|value| !valid_id(value))
        {
            return Some(jsonrpc_error(Value::Null, -32600, "Invalid Request"));
        }
        let method = object["method"].as_str().expect("validated method");
        if method == "notifications/cancelled" {
            self.record_cancellation(object.get("params"));
            return None;
        }
        let id = id?;
        if self.take_cancelled(&id) {
            return Some(jsonrpc_error(id, -32800, "Request cancelled"));
        }
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "d-token-headroom", "version": env!("CARGO_PKG_VERSION")},
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(
                json!({"tools": tool_definitions(self.retrieve_capability.is_some(), self.memory_capability.is_some())}),
            ),
            "tools/call" => self.call_tool(object.get("params")),
            _ => return Some(jsonrpc_error(id, -32601, "Method not found")),
        };
        Some(match result {
            Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
            Err(RpcFailure::InvalidParams) => jsonrpc_error(id, -32602, "Invalid params"),
            Err(RpcFailure::Tool(error)) => {
                json!({"jsonrpc": "2.0", "id": id, "result": error.content()})
            }
        })
    }

    fn record_cancellation(&self, params: Option<&Value>) {
        let Some(request_id) = params
            .and_then(Value::as_object)
            .and_then(|params| params.get("requestId"))
            .filter(|id| valid_id(id))
        else {
            return;
        };
        if let Ok(mut cancelled) = self.cancelled_request_ids.lock() {
            let request_id = request_id_key(request_id);
            if !cancelled.ids.insert(request_id.clone()) {
                return;
            }
            cancelled.arrival_order.push_back(request_id);
            while cancelled.arrival_order.len() > MAX_CANCELLED_REQUESTS {
                let Some(oldest) = cancelled.arrival_order.pop_front() else {
                    break;
                };
                cancelled.ids.remove(&oldest);
            }
        }
    }

    fn take_cancelled(&self, id: &Value) -> bool {
        self.cancelled_request_ids
            .lock()
            .map(|mut cancelled| {
                let request_id = request_id_key(id);
                if !cancelled.ids.remove(&request_id) {
                    return false;
                }
                cancelled.arrival_order.retain(|entry| entry != &request_id);
                true
            })
            .unwrap_or(false)
    }

    fn call_tool(&self, params: Option<&Value>) -> Result<Value, RpcFailure> {
        let request: ToolCall = serde_json::from_value(params.cloned().unwrap_or(Value::Null))
            .map_err(|_| RpcFailure::InvalidParams)?;
        let arguments = request.arguments.unwrap_or_else(|| json!({}));
        match request.name.as_str() {
            TOOL_COMPRESS => {
                let arguments: CompressArguments =
                    serde_json::from_value(arguments).map_err(|_| {
                        RpcFailure::Tool(McpToolError::invalid_arguments(
                            "invalid compress arguments",
                        ))
                    })?;
                let requested_policy = arguments
                    .policy
                    .as_deref()
                    .map(|value| {
                        parse_policy(value).ok_or_else(|| {
                            RpcFailure::Tool(McpToolError::invalid_arguments(
                                "invalid compression policy",
                            ))
                        })
                    })
                    .transpose()?;
                self.state
                    .mcp_compress(requested_policy, arguments.route_ref, arguments.body)
                    .map(tool_success)
                    .map_err(RpcFailure::Tool)
            }
            TOOL_STATS => {
                let _: EmptyArguments =
                    serde_json::from_value(arguments).map_err(|_| RpcFailure::InvalidParams)?;
                Ok(tool_success(self.state.mcp_stats()))
            }
            TOOL_RETRIEVE => self
                .retrieve_ccr(arguments)
                .map(tool_success)
                .map_err(RpcFailure::Tool),
            TOOL_DELETE => self
                .delete_ccr(arguments)
                .map(tool_success)
                .map_err(RpcFailure::Tool),
            TOOL_MEMORY_LIST => self
                .list_memory(arguments)
                .map(tool_success)
                .map_err(RpcFailure::Tool),
            TOOL_MEMORY_EXPORT => self
                .export_memory(arguments)
                .map(tool_success)
                .map_err(RpcFailure::Tool),
            TOOL_MEMORY_DELETE => self
                .delete_memory(arguments)
                .map(tool_success)
                .map_err(RpcFailure::Tool),
            TOOL_MEMORY_WRITE => self
                .write_memory(arguments)
                .map(tool_success)
                .map_err(RpcFailure::Tool),
            _ => Err(RpcFailure::Tool(McpToolError {
                code: "unknown_tool",
                message: "The requested d-token MCP tool is not available.",
            })),
        }
    }

    fn retrieve_ccr(&self, arguments: Value) -> Result<Value, McpToolError> {
        let Some(expected) = self.retrieve_capability.as_deref() else {
            return Err(McpToolError {
                code: "retrieve_authorization_unavailable",
                message: "Retrieval requires a managed d-token MCP capability.",
            });
        };
        let arguments: RetrieveArguments = serde_json::from_value(arguments)
            .map_err(|_| McpToolError::invalid_arguments("invalid retrieve arguments"))?;
        if !capability_matches(expected, &arguments.authorization) {
            return Err(McpToolError {
                code: "retrieve_authorization_denied",
                message: "The retrieval capability is invalid.",
            });
        }
        let bytes = self
            .state
            .retrieve_ccr_original_bound(&arguments.recovery_ref)
            .map_err(|_| McpToolError {
                code: "recovery_unavailable",
                message: "The requested recovery entry is unavailable.",
            })?;
        if bytes.len() > MAX_MCP_COMPRESS_BODY_BYTES {
            return Err(McpToolError::request_too_large());
        }
        let body = serde_json::from_slice::<Value>(&bytes).map_err(|_| McpToolError {
            code: "recovery_unavailable",
            message: "The requested recovery entry cannot be returned through MCP.",
        })?;
        Ok(
            json!({ "source": "dtoken-owned-runtime", "state": "retrieved", "recovery_ref": arguments.recovery_ref, "body": body }),
        )
    }

    fn delete_ccr(&self, arguments: Value) -> Result<Value, McpToolError> {
        let Some(expected) = self.retrieve_capability.as_deref() else {
            return Err(McpToolError {
                code: "retrieve_authorization_unavailable",
                message: "Deletion requires a managed d-token MCP capability.",
            });
        };
        let arguments: RetrieveArguments = serde_json::from_value(arguments)
            .map_err(|_| McpToolError::invalid_arguments("invalid delete arguments"))?;
        if !capability_matches(expected, &arguments.authorization) {
            return Err(McpToolError {
                code: "retrieve_authorization_denied",
                message: "The retrieval capability is invalid.",
            });
        }
        let receipt = self
            .state
            .delete_ccr_original(&arguments.recovery_ref)
            .map_err(|_| McpToolError {
                code: "recovery_unavailable",
                message: "The requested recovery entry is unavailable.",
            })?;
        Ok(json!(receipt))
    }

    fn authorized_memory_scope(
        &self,
        permission: MemoryCapabilityPermission,
    ) -> Result<MemoryAccessScope, McpToolError> {
        let Some(grant) = self.memory_capability.as_ref() else {
            return Err(McpToolError::memory_unavailable());
        };
        if !grant.permits(permission) {
            return Err(McpToolError::memory_scope_denied());
        }
        Ok(grant.scope())
    }

    fn list_memory(&self, arguments: Value) -> Result<Value, McpToolError> {
        let _: EmptyArguments = serde_json::from_value(arguments)
            .map_err(|_| McpToolError::invalid_arguments("invalid SharedContext list arguments"))?;
        let scope = self.authorized_memory_scope(MemoryCapabilityPermission::List)?;
        self.state.mcp_list_memory(&scope)
    }

    fn export_memory(&self, arguments: Value) -> Result<Value, McpToolError> {
        let arguments: MemoryExportArguments = serde_json::from_value(arguments).map_err(|_| {
            McpToolError::invalid_arguments("invalid SharedContext export arguments")
        })?;
        let scope = self.authorized_memory_scope(MemoryCapabilityPermission::Export)?;
        self.state
            .mcp_export_memory(&scope, arguments.include_tombstones)
    }

    fn delete_memory(&self, arguments: Value) -> Result<Value, McpToolError> {
        let arguments: MemoryDeleteArguments = serde_json::from_value(arguments).map_err(|_| {
            McpToolError::invalid_arguments("invalid SharedContext delete arguments")
        })?;
        let scope = self.authorized_memory_scope(MemoryCapabilityPermission::Delete)?;
        self.state.mcp_delete_memory(
            &scope,
            &arguments.memory_id,
            &arguments.expected_revision,
            &arguments.tombstone_revision,
        )
    }

    fn write_memory(&self, arguments: Value) -> Result<Value, McpToolError> {
        let arguments: MemoryWriteArguments = serde_json::from_value(arguments).map_err(|_| {
            McpToolError::invalid_arguments("invalid SharedContext write arguments")
        })?;
        let scope = self.authorized_memory_scope(MemoryCapabilityPermission::Write)?;
        let record = parse_memory_write_request(&arguments.record)
            .map_err(|_| McpToolError::invalid_arguments("invalid typed memory record"))?;
        self.state.mcp_write_memory(&scope, &record)
    }
}

pub async fn serve_stdio(state: AppState) -> io::Result<()> {
    let retrieve_capability = std::env::var("DTOKEN_MCP_RETRIEVE_CAPABILITY").ok();
    serve_jsonrpc_lines_with_capabilities(
        state,
        retrieve_capability,
        None,
        tokio::io::stdin(),
        tokio::io::stdout(),
    )
    .await
}

pub async fn serve_stdio_with_memory_capability(
    state: AppState,
    capability: ResolvedMemoryCapability,
) -> io::Result<()> {
    let retrieve_capability = std::env::var("DTOKEN_MCP_RETRIEVE_CAPABILITY").ok();
    serve_jsonrpc_lines_with_capabilities(
        state,
        retrieve_capability,
        Some(ManagedMemoryCapability {
            resolved: capability,
        }),
        tokio::io::stdin(),
        tokio::io::stdout(),
    )
    .await
}

pub async fn serve_jsonrpc_lines<R, W>(state: AppState, input: R, output: W) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    serve_jsonrpc_lines_with_capabilities(state, None, None, input, output).await
}

async fn serve_jsonrpc_lines_with_capabilities<R, W>(
    state: AppState,
    retrieve_capability: Option<String>,
    memory_capability: Option<ManagedMemoryCapability>,
    input: R,
    mut output: W,
) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let server = McpServer::new_with_capabilities(state, retrieve_capability, memory_capability);
    let mut input = BufReader::new(input);
    while let Some(frame) = read_bounded_line(&mut input, MAX_MCP_MESSAGE_BYTES).await? {
        let response = match frame {
            StdioFrame::TooLarge => Some(jsonrpc_error(Value::Null, -32600, "Request too large")),
            StdioFrame::Line(line) => match serde_json::from_slice(&line) {
                Ok(request) => server.handle_jsonrpc(request),
                Err(_) => Some(jsonrpc_error(Value::Null, -32700, "Parse error")),
            },
        };
        if let Some(response) = response {
            let encoded =
                serde_json::to_vec(&response).expect("MCP responses contain serializable values");
            output.write_all(&encoded).await?;
            output.write_all(b"\n").await?;
            output.flush().await?;
        }
    }
    Ok(())
}

enum StdioFrame {
    Line(Vec<u8>),
    TooLarge,
}

async fn read_bounded_line<R>(
    reader: &mut R,
    maximum_bytes: usize,
) -> io::Result<Option<StdioFrame>>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    let mut too_large = false;
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return if line.is_empty() && !too_large {
                Ok(None)
            } else if too_large {
                Ok(Some(StdioFrame::TooLarge))
            } else {
                Ok(Some(StdioFrame::Line(line)))
            };
        }

        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let available = newline.unwrap_or(buffer.len());
        if !too_large {
            if line.len().saturating_add(available) > maximum_bytes {
                too_large = true;
            } else {
                line.extend_from_slice(&buffer[..available]);
            }
        }
        reader.consume(available + usize::from(newline.is_some()));

        if newline.is_some() {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(if too_large {
                StdioFrame::TooLarge
            } else {
                StdioFrame::Line(line)
            }));
        }
    }
}

#[derive(Debug)]
enum RpcFailure {
    InvalidParams,
    Tool(McpToolError),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCall {
    name: String,
    #[serde(default)]
    arguments: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompressArguments {
    body: Value,
    #[serde(default)]
    policy: Option<String>,
    route_ref: McpRouteReference,
}

/// A constrained route assertion from the MCP client. Provider identity,
/// upstream URL, credentials and model all remain runtime-owned values.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpRouteReference {
    pub(crate) route_id: String,
    pub(crate) input_protocol: Protocol,
    pub(crate) endpoint: OpticalEndpoint,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArguments {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetrieveArguments {
    recovery_ref: String,
    authorization: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryExportArguments {
    #[serde(default)]
    include_tombstones: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryDeleteArguments {
    memory_id: String,
    expected_revision: String,
    tombstone_revision: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryWriteArguments {
    record: Value,
}

fn valid_capability(value: &str) -> bool {
    value.len() >= 32 && value.len() <= 512 && !value.contains(['\n', '\r', '\0'])
}

fn capability_matches(expected: &str, presented: &str) -> bool {
    if expected.len() != presented.len() {
        return false;
    }
    expected
        .bytes()
        .zip(presented.bytes())
        .fold(0_u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

fn parse_policy(policy: &str) -> Option<CompressionPolicy> {
    match policy {
        "off" => Some(CompressionPolicy::Off),
        "safe" => Some(CompressionPolicy::Safe),
        "balanced" => Some(CompressionPolicy::Balanced),
        "aggressive" => Some(CompressionPolicy::Aggressive),
        "max" => Some(CompressionPolicy::Max),
        _ => None,
    }
}

fn valid_id(id: &Value) -> bool {
    id.is_string() || id.is_i64() || id.is_u64() || id.is_null()
}

fn request_id_key(id: &Value) -> String {
    serde_json::to_string(id).expect("JSON-RPC ids serialize")
}

fn jsonrpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    })
}

fn tool_success(value: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&value).expect("MCP tool results serialize"),
        }]
    })
}

fn tool_definitions(retrieve_available: bool, memory_available: bool) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": TOOL_COMPRESS,
            "description": "Compress an OpenAI-compatible JSON request with the d-token-owned safe pipeline. The request is not persisted by this tool.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["body", "route_ref"],
                "properties": {
                    "body": {"type": "object"},
                    "policy": {"type": "string", "enum": ["off", "safe", "balanced", "aggressive", "max"]},
                    "route_ref": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["route_id", "input_protocol", "endpoint"],
                        "properties": {
                            "route_id": {"type": "string", "minLength": 1},
                            "input_protocol": {"type": "string", "enum": ["open_ai", "anthropic"]},
                            "endpoint": {"type": "string", "enum": ["open_ai_chat_completions", "open_ai_responses", "anthropic_messages"]}
                        }
                    }
                }
            }
        }),
        json!({
            "name": TOOL_STATS,
            "description": "Read bounded, redacted d-token-owned compression statistics.",
            "inputSchema": {"type": "object", "additionalProperties": false}
        }),
    ];
    if retrieve_available {
        tools.push(json!({
            "name": TOOL_RETRIEVE,
            "description": "Retrieve one encrypted CCR original after presenting the managed d-token MCP capability.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["recovery_ref", "authorization"],
                "properties": {"recovery_ref": {"type": "string", "minLength": 1}, "authorization": {"type": "string", "minLength": 32}}
            }
        }));
        tools.push(json!({
            "name": TOOL_DELETE,
            "description": "Permanently delete one encrypted CCR original after presenting the managed d-token MCP capability.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["recovery_ref", "authorization"],
                "properties": {"recovery_ref": {"type": "string", "minLength": 1}, "authorization": {"type": "string", "minLength": 32}}
            }
        }));
    }
    if memory_available {
        tools.push(json!({
            "name": TOOL_MEMORY_LIST,
            "description": "List only typed SharedContext records visible to the managed project, Agent, and session scope.",
            "inputSchema": {"type": "object", "additionalProperties": false}
        }));
        tools.push(json!({
            "name": TOOL_MEMORY_EXPORT,
            "description": "Export only typed SharedContext records visible to the managed project, Agent, and session scope.",
            "inputSchema": {
                "type": "object", "additionalProperties": false,
                "properties": {
                    "include_tombstones": {"type": "boolean"}
                }
            }
        }));
        tools.push(json!({
            "name": TOOL_MEMORY_DELETE,
            "description": "Tombstone one owned SharedContext record using an exact revision-bound managed scope.",
            "inputSchema": {
                "type": "object", "additionalProperties": false,
                "required": ["memory_id", "expected_revision", "tombstone_revision"],
                "properties": {
                    "memory_id": {"type": "string", "minLength": 1},
                    "expected_revision": {"type": "string", "minLength": 1},
                    "tombstone_revision": {"type": "string", "minLength": 1}
                }
            }
        }));
        tools.push(json!({
            "name": TOOL_MEMORY_WRITE,
            "description": "Write one typed SharedContext record only when its project, Agent, and session exactly match the managed scope.",
            "inputSchema": {
                "type": "object", "additionalProperties": false,
                "required": ["record"],
                "properties": {
                    "record": {"type": "object"}
                }
            }
        }));
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{routed_openai_payload, OpenAiChatRequest};
    use dtoken_config::DtokenConfig;
    use dtoken_context_store::{MemoryProvenance, MemoryWriteRequest};
    use dtoken_types::{ChainMode, ProviderKind, ProviderProfile, RouteProfile};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn request(id: u64, method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }

    fn tool_text(response: Value) -> String {
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("MCP text content")
            .to_string()
    }

    fn mcp_route_config() -> DtokenConfig {
        DtokenConfig {
            providers: vec![ProviderProfile {
                id: "mcp-provider".to_string(),
                name: "MCP provider".to_string(),
                kind: ProviderKind::Custom,
                protocol: Protocol::OpenAi,
                base_url: "http://127.0.0.1:9/v1".to_string(),
                api_key_ref: None,
                models: vec!["mcp-model".to_string()],
                timeout_ms: 100,
                healthcheck_path: None,
            }],
            routes: vec![RouteProfile {
                id: "mcp-route".to_string(),
                name: "MCP route".to_string(),
                input_protocol: Protocol::OpenAi,
                instance_id: "mcp-instance".to_string(),
                target_provider_id: "mcp-provider".to_string(),
                fallback_provider_id: None,
                chain_mode: ChainMode::Direct,
                compression_policy: CompressionPolicy::Balanced,
                bind_host: "127.0.0.1".to_string(),
                local_port: 18081,
            }],
            instances: Vec::new(),
            provider_cache_policies: Vec::new(),
            output_shaping_policies: Vec::new(),
            optical: Default::default(),
        }
    }

    fn mcp_route_ref() -> Value {
        json!({
            "route_id": "mcp-route",
            "input_protocol": "open_ai",
            "endpoint": "open_ai_chat_completions"
        })
    }

    fn memory_write(
        project_id: &str,
        agent_id: &str,
        session_id: &str,
        summary: &str,
        entry_revision: &str,
    ) -> MemoryWriteRequest {
        MemoryWriteRequest {
            project_id: project_id.to_string(),
            agent_id: Some(agent_id.to_string()),
            session_id: Some(session_id.to_string()),
            kind: "preference".to_string(),
            summary: summary.to_string(),
            keywords: vec!["terse".to_string()],
            dedup_key: format!("{agent_id}-{session_id}"),
            entry_revision: entry_revision.to_string(),
            provenance: MemoryProvenance {
                source: "managed-test".to_string(),
                source_revision: "e".repeat(64),
                observed_at_ms: 1,
            },
        }
    }

    #[test]
    fn initialize_lists_tools_and_executes_compress_and_stats() {
        let server = McpServer::new(AppState::new(mcp_route_config()));
        let initialized = server
            .handle_jsonrpc(request(
                1,
                "initialize",
                json!({"protocolVersion": PROTOCOL_VERSION}),
            ))
            .expect("initialize response");
        assert_eq!(initialized["result"]["protocolVersion"], PROTOCOL_VERSION);

        let listed = server
            .handle_jsonrpc(request(2, "tools/list", json!({})))
            .expect("tools/list response");
        assert_eq!(
            listed["result"]["tools"].as_array().expect("tools").len(),
            2
        );

        let compressed = server
            .handle_jsonrpc(request(3, "tools/call", json!({
                "name": TOOL_COMPRESS,
                "arguments": {
                    "policy": "balanced",
                    "route_ref": mcp_route_ref(),
                    "body": {"model": "mcp-model", "messages": [{"role": "user", "content": "hello"}]}
                }
            })))
            .expect("compress response");
        let compressed = tool_text(compressed);
        assert!(compressed.contains("dtoken-owned-runtime"));
        let compressed: Value = serde_json::from_str(&compressed).expect("compress result JSON");
        assert_eq!(compressed["state"], "passthrough");
        assert_eq!(
            compressed["fallback_reason_code"],
            "compression_not_applicable"
        );
        assert_eq!(compressed["decision"]["state"], "resolved");
        assert_eq!(compressed["decision"]["route_id"], "mcp-route");
        assert_eq!(compressed["decision"]["provider_id"], "mcp-provider");
        assert_eq!(compressed["decision"]["model"], "mcp-model");
        assert_eq!(compressed["evidence"]["endpoint_identity"], "validated");
        assert!(!compressed.to_string().contains("127.0.0.1:9"));

        let stats = server
            .handle_jsonrpc(request(
                4,
                "tools/call",
                json!({"name": TOOL_STATS, "arguments": {}}),
            ))
            .expect("stats response");
        assert!(tool_text(stats).contains("raw_available"));
    }

    #[test]
    fn retrieval_and_invalid_arguments_fail_closed_without_echoing_input() {
        let server = McpServer::new(AppState::new(DtokenConfig::empty()));
        let secret = "MCP_TEST_SECRET_MUST_NOT_ECHO";
        let invalid = server
            .handle_jsonrpc(request(
                1,
                "tools/call",
                json!({
                    "name": TOOL_COMPRESS,
                    "arguments": {"policy": "not-a-policy", "route_ref": mcp_route_ref(), "body": {"model": secret}}
                }),
            ))
            .expect("invalid tool response");
        assert_eq!(invalid["result"]["isError"], true);
        assert!(!invalid.to_string().contains(secret));

        let oversized = server
            .handle_jsonrpc(request(
                3,
                "tools/call",
                json!({
                    "name": TOOL_COMPRESS,
                    "arguments": {
                        "body": {
                            "model": secret,
                            "messages": [{"role": "user", "content": "x".repeat(MAX_MCP_COMPRESS_BODY_BYTES)}]
                        },
                        "route_ref": mcp_route_ref()
                    }
                }),
            ))
            .expect("oversized tool response");
        assert_eq!(oversized["result"]["isError"], true);
        assert!(tool_text(oversized.clone()).contains("request_too_large"));
        assert!(!oversized.to_string().contains(secret));

        let retrieve = server
            .handle_jsonrpc(request(
                2,
                "tools/call",
                json!({
                    "name": TOOL_RETRIEVE,
                    "arguments": {"recovery_ref": secret}
                }),
            ))
            .expect("retrieve response");
        assert_eq!(retrieve["result"]["isError"], true);
        assert!(retrieve
            .to_string()
            .contains("retrieve_authorization_unavailable"));
        assert!(!retrieve.to_string().contains(secret));
    }

    #[test]
    fn compression_requires_a_configured_route_and_validates_the_route_assertion() {
        let secret = "MCP_ROUTE_SECRET_MUST_NOT_ECHO";
        let server = McpServer::new(AppState::new(mcp_route_config()));
        let body = json!({"model": "mcp-model", "messages": [{"role": "user", "content": secret}]});

        let missing = server
            .handle_jsonrpc(request(
                1,
                "tools/call",
                json!({
                    "name": TOOL_COMPRESS,
                    "arguments": {"body": body.clone()}
                }),
            ))
            .expect("missing route response");
        assert_eq!(missing["result"]["isError"], true);
        assert!(tool_text(missing.clone()).contains("invalid_arguments"));
        assert!(!missing.to_string().contains(secret));

        let unknown = server
            .handle_jsonrpc(request(2, "tools/call", json!({
                "name": TOOL_COMPRESS,
                "arguments": {
                    "route_ref": {"route_id": "unknown", "input_protocol": "open_ai", "endpoint": "open_ai_chat_completions"},
                    "body": body.clone()
                }
            })))
            .expect("unknown route response");
        assert_eq!(unknown["result"]["isError"], true);
        assert!(tool_text(unknown.clone()).contains("route_context_unavailable"));
        assert!(!unknown.to_string().contains(secret));

        let endpoint_mismatch = server
            .handle_jsonrpc(request(3, "tools/call", json!({
                "name": TOOL_COMPRESS,
                "arguments": {
                    "route_ref": {"route_id": "mcp-route", "input_protocol": "open_ai", "endpoint": "open_ai_responses"},
                    "body": body
                }
            })))
            .expect("endpoint mismatch response");
        assert_eq!(endpoint_mismatch["result"]["isError"], true);
        assert!(tool_text(endpoint_mismatch).contains("route_context_mismatch"));

        let unlisted_model = server
            .handle_jsonrpc(request(
                4,
                "tools/call",
                json!({
                    "name": TOOL_COMPRESS,
                    "arguments": {
                        "route_ref": mcp_route_ref(),
                        "body": {"model": "not-configured", "messages": []}
                    }
                }),
            ))
            .expect("unlisted model response");
        assert_eq!(unlisted_model["result"]["isError"], true);
        assert!(tool_text(unlisted_model).contains("route_context_unavailable"));

        let policy_mismatch = server
            .handle_jsonrpc(request(
                5,
                "tools/call",
                json!({
                    "name": TOOL_COMPRESS,
                    "arguments": {
                        "policy": "safe",
                        "route_ref": mcp_route_ref(),
                        "body": {"model": "mcp-model", "messages": []}
                    }
                }),
            ))
            .expect("policy mismatch response");
        assert_eq!(policy_mismatch["result"]["isError"], true);
        assert!(tool_text(policy_mismatch).contains("route_policy_mismatch"));
    }

    #[test]
    fn retrieval_requires_a_managed_capability_and_returns_only_to_an_authorized_tool_call() {
        let directory =
            std::env::temp_dir().join(format!("dtoken-mcp-ccr-{}", uuid::Uuid::new_v4()));
        let state =
            AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &directory, [9_u8; 32])
                .expect("state with CCR store");
        let openai_request = OpenAiChatRequest {
            model: Some("mcp-retrieve-model".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "authorized original"
            })]),
            stream: Some(false),
        };
        let (_response, decision, mut request_log, event) = routed_openai_payload(&openai_request);
        request_log.transforms_applied = vec!["content_router:json".to_string()];
        let original = br#"{"model":"mcp-retrieve-model","messages":[{"role":"user","content":"authorized original"}]}"#;
        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, original)
            .expect("prepare CCR lifecycle");
        let recovery_ref = format!("ccr:{}", decision.request_id);
        state.record_routed_request_with_ccr_receipt(&decision, request_log, event, &prepared);
        let capability = "c".repeat(48);
        let server =
            McpServer::new_with_retrieve_capability(state.clone(), Some(capability.clone()));

        let listed = server
            .handle_jsonrpc(request(1, "tools/list", json!({})))
            .expect("tool list");
        assert_eq!(listed["result"]["tools"].as_array().map(Vec::len), Some(4));

        let denied = server
            .handle_jsonrpc(request(
                2,
                "tools/call",
                json!({
                    "name": TOOL_RETRIEVE,
                    "arguments": {"recovery_ref": recovery_ref, "authorization": "x".repeat(48)}
                }),
            ))
            .expect("denied response");
        assert_eq!(denied["result"]["isError"], true);
        assert!(!denied.to_string().contains("authorized original"));

        let retrieved = server
            .handle_jsonrpc(request(
                3,
                "tools/call",
                json!({
                    "name": TOOL_RETRIEVE,
                    "arguments": {"recovery_ref": recovery_ref, "authorization": capability}
                }),
            ))
            .expect("retrieve response");
        let result: Value = serde_json::from_str(&tool_text(retrieved)).expect("tool result JSON");
        assert_eq!(result["state"], "retrieved");
        assert_eq!(
            result["body"],
            serde_json::from_slice::<Value>(original).expect("original JSON")
        );

        // Fail-closed bound retrieval: a ref without any durable lifecycle
        // metadata (even though the encrypted envelope exists) is rejected
        // before any byte is decrypted.
        let untracked_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        state
            .ccr_private_store
            .as_ref()
            .expect("private store")
            .store(&untracked_ref, b"{\"content\":\"UNTRAKCED_SECRET\"}")
            .expect("store untracked envelope");
        let untracked = server
            .handle_jsonrpc(request(
                4,
                "tools/call",
                json!({
                    "name": TOOL_RETRIEVE,
                    "arguments": {"recovery_ref": untracked_ref, "authorization": capability}
                }),
            ))
            .expect("untracked response");
        assert_eq!(untracked["result"]["isError"], true);
        assert!(tool_text(untracked).contains("recovery_unavailable"));

        // After deletion the lifecycle tombstone makes bound retrieval reject
        // the ref even though the caller holds the capability.
        let deleted = server
            .handle_jsonrpc(request(
                5,
                "tools/call",
                json!({
                    "name": TOOL_DELETE,
                    "arguments": {"recovery_ref": recovery_ref, "authorization": capability}
                }),
            ))
            .expect("delete response");
        assert_eq!(deleted["result"]["isError"], Value::Null);
        let after_delete = server
            .handle_jsonrpc(request(
                6,
                "tools/call",
                json!({
                    "name": TOOL_RETRIEVE,
                    "arguments": {"recovery_ref": recovery_ref, "authorization": capability}
                }),
            ))
            .expect("post-delete response");
        assert_eq!(after_delete["result"]["isError"], true);
        assert!(tool_text(after_delete.clone()).contains("recovery_unavailable"));
        assert!(!after_delete.to_string().contains("authorized original"));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn managed_ccr_delete_returns_a_redacted_idempotent_tombstone_receipt() {
        let directory =
            std::env::temp_dir().join(format!("dtoken-mcp-ccr-delete-{}", uuid::Uuid::new_v4()));
        let state =
            AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &directory, [10_u8; 32])
                .expect("state with CCR store");
        let openai_request = OpenAiChatRequest {
            model: Some("mcp-delete-model".to_string()),
            messages: Some(vec![
                json!({"role": "user", "content": "MCP_DELETE_SECRET"}),
            ]),
            stream: Some(false),
        };
        let (_response, decision, mut request_log, event) = routed_openai_payload(&openai_request);
        request_log.transforms_applied = vec!["content_router:json".to_string()];
        let original = br#"{"messages":[{"content":"MCP_DELETE_SECRET"}]}"#;
        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, original)
            .expect("prepare CCR");
        let recovery_ref = format!("ccr:{}", decision.request_id);
        state.record_routed_request_with_ccr_receipt(&decision, request_log, event, &prepared);
        let capability = "d".repeat(48);
        let server =
            McpServer::new_with_retrieve_capability(state.clone(), Some(capability.clone()));

        let deleted = server
            .handle_jsonrpc(request(
                1,
                "tools/call",
                json!({
                    "name": TOOL_DELETE,
                    "arguments": {"recovery_ref": recovery_ref, "authorization": capability}
                }),
            ))
            .expect("deleted response");
        let deleted: Value = serde_json::from_str(&tool_text(deleted)).expect("deleted receipt");
        assert_eq!(deleted["state"], "deleted");
        assert_eq!(deleted["raw_available"], false);
        assert_eq!(deleted["raw_returned"], false);
        assert!(!deleted.to_string().contains("MCP_DELETE_SECRET"));

        let repeated = server
            .handle_jsonrpc(request(
                2,
                "tools/call",
                json!({
                    "name": TOOL_DELETE,
                    "arguments": {"recovery_ref": recovery_ref, "authorization": capability}
                }),
            ))
            .expect("repeated deleted response");
        let repeated: Value =
            serde_json::from_str(&tool_text(repeated)).expect("idempotent receipt");
        assert_eq!(repeated["state"], "already_deleted");
        assert!(state.retrieve_ccr_original(&recovery_ref).is_err());
        let metadata = state
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("tombstone metadata");
        assert_eq!(metadata["state"], "deleted");
        assert!(!metadata.to_string().contains("MCP_DELETE_SECRET"));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn managed_memory_capability_is_exactly_scoped_and_revision_bound() {
        let directory =
            std::env::temp_dir().join(format!("dtoken-mcp-memory-{}", uuid::Uuid::new_v4()));
        let state = AppState::new_persistent(DtokenConfig::empty(), &directory);
        let owner = state
            .record_memory(&memory_write(
                "project-a",
                "opencode",
                "session-a",
                "visible typed preference",
                &"a".repeat(64),
            ))
            .expect("write owner memory");
        state
            .record_memory(&memory_write(
                "project-a",
                "opencode",
                "session-b",
                "other session typed preference",
                &"c".repeat(64),
            ))
            .expect("write other-session memory");
        state
            .record_memory(&memory_write(
                "project-a",
                "claude-code",
                "session-a",
                "other agent typed preference",
                &"d".repeat(64),
            ))
            .expect("write other-agent memory");
        let capability = ResolvedMemoryCapability {
            transaction_id: format!("memcap-{}", uuid::Uuid::new_v4()),
            scope: MemoryAccessScope {
                project_id: "project-a".to_string(),
                agent_id: "opencode".to_string(),
                session_id: Some("session-a".to_string()),
            },
            route_binding: dtoken_context_store::MemoryCapabilityRouteBinding {
                route_id: "route-a".to_string(),
                provider_id: "provider-a".to_string(),
                model: "model-a".to_string(),
                config_revision: "e".repeat(64),
            },
            permissions: vec![
                MemoryCapabilityPermission::List,
                MemoryCapabilityPermission::Export,
                MemoryCapabilityPermission::Write,
                MemoryCapabilityPermission::Delete,
            ],
            source_revision: "managed-test".to_string(),
            expires_at_ms: dtoken_types::time::now_ms() + 60_000,
        };
        let server = McpServer::new_with_capabilities(
            state,
            None,
            Some(ManagedMemoryCapability {
                resolved: capability.clone(),
            }),
        );

        let tools = server
            .handle_jsonrpc(request(1, "tools/list", json!({})))
            .expect("tools list");
        assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(6));

        let limited = McpServer::new_with_capabilities(
            server.state.clone(),
            None,
            Some(ManagedMemoryCapability {
                resolved: ResolvedMemoryCapability {
                    permissions: vec![MemoryCapabilityPermission::List],
                    ..capability
                },
            }),
        );
        let denied = limited
            .handle_jsonrpc(request(
                2,
                "tools/call",
                json!({"name": TOOL_MEMORY_EXPORT, "arguments": {}}),
            ))
            .expect("denied export");
        assert_eq!(denied["result"]["isError"], true);
        assert!(tool_text(denied).contains("memory_access_denied"));

        let scope_mismatch = server
            .handle_jsonrpc(request(
                21,
                "tools/call",
                json!({
                    "name": TOOL_MEMORY_WRITE,
                    "arguments": {
                        "record": {
                            "project_id": "project-a", "agent_id": "claude-code", "session_id": "session-a",
                            "kind": "preference", "summary": "must not be written", "keywords": ["terse"],
                            "dedup_key": "mismatch", "entry_revision": "f".repeat(64),
                            "provenance": {"source": "managed-test", "source_revision": "e".repeat(64), "observed_at_ms": 2}
                        }
                    }
                }),
            ))
            .expect("scope mismatch response");
        assert_eq!(scope_mismatch["result"]["isError"], true);
        assert!(tool_text(scope_mismatch).contains("memory_access_denied"));

        let written = server
            .handle_jsonrpc(request(
                22,
                "tools/call",
                json!({
                    "name": TOOL_MEMORY_WRITE,
                    "arguments": {
                        "record": {
                            "project_id": "project-a", "agent_id": "opencode", "session_id": "session-a",
                            "kind": "preference", "summary": "written inside the managed scope", "keywords": ["terse"],
                            "dedup_key": "managed-write", "entry_revision": "f".repeat(64),
                            "provenance": {"source": "managed-test", "source_revision": "e".repeat(64), "observed_at_ms": 2}
                        }
                    }
                }),
            ))
            .expect("managed write response");
        assert_eq!(written["result"]["isError"], Value::Null);

        let listed = server
            .handle_jsonrpc(request(
                3,
                "tools/call",
                json!({"name": TOOL_MEMORY_LIST, "arguments": {}}),
            ))
            .expect("scoped list");
        let listed: Value = serde_json::from_str(&tool_text(listed)).expect("list result JSON");
        assert_eq!(listed["state"], "listed");
        assert_eq!(listed["count"], 2);
        assert!(listed["entries"].as_array().is_some_and(|entries| entries
            .iter()
            .any(|entry| entry["memory_id"] == owner["memory_id"])));
        assert!(!listed.to_string().contains("other session"));
        assert!(!listed.to_string().contains("other agent"));

        let expected_revision = owner["entry_revision"].as_str().expect("owner revision");
        let deleted = server
            .handle_jsonrpc(request(
                4,
                "tools/call",
                json!({
                    "name": TOOL_MEMORY_DELETE,
                    "arguments": {
                        "memory_id": owner["memory_id"],
                        "expected_revision": expected_revision,
                        "tombstone_revision": "b".repeat(64)
                    }
                }),
            ))
            .expect("delete result");
        let deleted: Value = serde_json::from_str(&tool_text(deleted)).expect("delete result JSON");
        assert_eq!(deleted["state"], "deleted");

        let exported = server
            .handle_jsonrpc(request(
                5,
                "tools/call",
                json!({"name": TOOL_MEMORY_EXPORT, "arguments": {"include_tombstones": true}}),
            ))
            .expect("export result");
        let exported: Value =
            serde_json::from_str(&tool_text(exported)).expect("export result JSON");
        assert_eq!(exported["count"], 2);
        assert!(exported["entries"]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| {
                entry["memory_id"] == owner["memory_id"]
                    && entry["state"] == "deleted"
                    && entry["summary"] == Value::Null
            })));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn cancellation_is_typed_and_bounded() {
        let server = McpServer::new(AppState::new(DtokenConfig::empty()));
        assert!(server
            .handle_jsonrpc(json!({
                "jsonrpc": "2.0",
                "method": "notifications/cancelled",
                "params": {"requestId": 99}
            }))
            .is_none());
        let cancelled = server
            .handle_jsonrpc(request(99, "ping", json!({})))
            .expect("cancelled response");
        assert_eq!(cancelled["error"]["code"], -32800);
    }

    #[test]
    fn cancellation_bookkeeping_evicts_in_arrival_order() {
        let server = McpServer::new(AppState::new(DtokenConfig::empty()));
        for request_id in 0..=MAX_CANCELLED_REQUESTS {
            assert!(server
                .handle_jsonrpc(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/cancelled",
                    "params": {"requestId": request_id}
                }))
                .is_none());
        }

        let first = server
            .handle_jsonrpc(request(0, "ping", json!({})))
            .expect("first response");
        assert_eq!(first["result"], json!({}));
        let last = server
            .handle_jsonrpc(request(MAX_CANCELLED_REQUESTS as u64, "ping", json!({})))
            .expect("last response");
        assert_eq!(last["error"]["code"], -32800);
    }

    #[tokio::test]
    async fn stdio_transport_bounds_oversized_frames_and_continues() {
        let state = AppState::new(DtokenConfig::empty());
        let (mut input_writer, input_reader) = tokio::io::duplex(64 * 1024);
        let (output_writer, mut output_reader) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(serve_jsonrpc_lines(state, input_reader, output_writer));

        input_writer
            .write_all(
                format!(
                    "{}\n",
                    request(
                        1,
                        "initialize",
                        json!({"protocolVersion": PROTOCOL_VERSION})
                    )
                )
                .as_bytes(),
            )
            .await
            .expect("write initialize");
        input_writer
            .write_all(&vec![b'x'; MAX_MCP_MESSAGE_BYTES + 1])
            .await
            .expect("write oversized frame");
        input_writer
            .write_all(b"\n")
            .await
            .expect("terminate frame");
        input_writer
            .write_all(format!("{}\n", request(2, "tools/list", json!({}))).as_bytes())
            .await
            .expect("write tools list");
        drop(input_writer);

        server
            .await
            .expect("stdio task join")
            .expect("stdio server result");
        let mut response = String::new();
        output_reader
            .read_to_string(&mut response)
            .await
            .expect("read responses");
        let responses = response
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("JSON-RPC response"))
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 3);
        assert_eq!(responses[0]["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(responses[1]["error"]["code"], -32600);
        assert_eq!(
            responses[2]["result"]["tools"].as_array().map(Vec::len),
            Some(2)
        );
    }
}
