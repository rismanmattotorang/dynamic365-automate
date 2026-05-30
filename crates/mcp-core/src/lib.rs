//! MCP core: JSON-RPC 2.0 framing and MCP 2025-06-18 protocol types.

pub mod error;
pub mod jsonrpc;
pub mod protocol;

pub use error::{Error, Result};
pub use jsonrpc::{ErrorObject, Id, Message, Notification, Request, Response};
pub use protocol::{
    CallToolParams,
    CallToolResult,
    CancelledParams,
    ClientCapabilities,
    CompleteParams,
    CompleteResult,
    CompletionArgumentRef,
    CompletionData,
    CompletionRef,
    ElicitationAction,
    ElicitationParams,
    ElicitationResult,
    GetPromptParams,
    GetPromptResult,
    Implementation,
    InitializeParams,
    InitializeResult,
    ListPromptsResult,
    ListResourcesResult,
    ListToolsResult,
    // MCP 2025-06-18 optional utilities.
    LogLevel,
    LogMessageParams,
    ProgressParams,
    ProgressToken,
    Prompt,
    PromptArgument,
    PromptMessage,
    ReadResourceParams,
    ReadResourceResult,
    Resource,
    ResourceContents,
    Role,
    ServerCapabilities,
    SetLevelParams,
    Tool,
    ToolContent,
    ToolInputSchema,
    PROTOCOL_VERSION,
};
