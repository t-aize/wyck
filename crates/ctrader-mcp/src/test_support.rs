//! In-process mock MCP server for tests (behind the `test-support` feature): a real HTTP
//! round-trip with no live network dependency, built on `rmcp`'s own server-side
//! streamable-HTTP implementation, the same pattern `rmcp` uses in its own test suite
//! (`rmcp-*/tests/test_server_discover_http.rs`).
//!
//! Exposed as a library module (rather than living only under `tests/`) so that other
//! workspace crates, notably `wyck-engine`, can drive `RemoteClient`/`LocalClient`
//! against scripted server behavior in their own tests. Enable it from a dev-dependency:
//!
//! ```toml
//! [dev-dependencies]
//! ctrader-mcp = { workspace = true, features = ["test-support"] }
//! ```
//!
//! Not part of the crate's stable API surface: it exists for tests only.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData as McpError,
    Implementation, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
    Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ServerHandler, model::JsonObject};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// One tool's behavior in a [`MockMcpServer`]: given the incoming `tools/call`
/// arguments, produce the [`CallToolResult`] the mock server sends back.
pub type ToolHandler = Arc<dyn Fn(Option<JsonObject>) -> CallToolResult + Send + Sync>;

/// A `ServerHandler` whose `tools/list` and `tools/call` are entirely driven by a
/// caller-supplied map of tool name -> [`ToolHandler`] closure, so each test can stand up
/// exactly the server behavior it wants to exercise (a canned success payload, an
/// error envelope, an assertion on the arguments actually received, a handler that fails
/// N times before succeeding, ...).
#[derive(Clone, Default)]
pub struct MockMcpServer {
    tools: Arc<HashMap<String, ToolHandler>>,
}

impl MockMcpServer {
    pub fn builder() -> MockMcpServerBuilder {
        MockMcpServerBuilder::default()
    }
}

#[derive(Default)]
pub struct MockMcpServerBuilder {
    tools: HashMap<String, ToolHandler>,
}

impl MockMcpServerBuilder {
    /// Registers `tool`, backed by `handler`. Panics if `tool` is already registered
    /// (a test bug, not a runtime condition).
    #[must_use]
    pub fn with_tool(
        mut self,
        tool: impl Into<String>,
        handler: impl Fn(Option<JsonObject>) -> CallToolResult + Send + Sync + 'static,
    ) -> Self {
        let tool = tool.into();
        assert!(
            self.tools.insert(tool.clone(), Arc::new(handler)).is_none(),
            "tool `{tool}` already registered on this MockMcpServer"
        );
        self
    }

    pub fn build(self) -> MockMcpServer {
        MockMcpServer {
            tools: Arc::new(self.tools),
        }
    }
}

impl ServerHandler for MockMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("mock-ctrader-mcp", "0.0.0"))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let result = match self.tools.get(request.name.as_ref()) {
            Some(handler) => handler(request.arguments),
            None => CallToolResult::error(vec![ContentBlock::text(format!(
                "mock server has no handler registered for tool `{}`",
                request.name
            ))]),
        };
        Ok(result.into())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = self
            .tools
            .keys()
            .map(|name| Tool::new(name.clone(), "", JsonObject::new()))
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }
}

/// Spins up `server` on `127.0.0.1:<ephemeral port>` and returns the `/mcp` URL to
/// connect [`crate::transport::McpSession::connect`] to, plus the background
/// [`JoinHandle`] serving it (call `.abort()` when the test is done with it).
pub async fn spawn_mock_mcp_server(server: MockMcpServer) -> (String, JoinHandle<()>) {
    let config = StreamableHttpServerConfig::default();
    let service: StreamableHttpService<MockMcpServer, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(server.clone()), Default::default(), config);
    let router = Router::new().nest_service("/mcp", service);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock MCP server should bind to an ephemeral port");
    let address = listener
        .local_addr()
        .expect("bound listener has an address");

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    (format!("http://{address}/mcp"), handle)
}

/// Spins up a raw `axum` router with no MCP semantics at all: for tests that only care
/// about what happens at the HTTP layer (which headers a request carried, what status a
/// response returned) below where MCP framing would even apply. Mirrors the pattern in
/// `rmcp`'s own `tests/test_streamable_http_get_stream_auth_challenge.rs`.
pub async fn spawn_raw_http_server(router: Router) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("raw mock server should bind to an ephemeral port");
    let address = listener
        .local_addr()
        .expect("bound listener has an address");

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    (format!("http://{address}/mcp"), handle)
}
