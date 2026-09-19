//! Regression coverage for the `ping` bug found by a live probe against both server
//! families (`examples/probe_remote.rs`, `examples/probe_local.rs`): neither Remote nor
//! Local advertises `"ping"` in `tools/list`, because MCP's `ping` is a protocol-level
//! request (`ClientRequest::PingRequest`), not a tool: `McpSession::ping`/
//! `RemoteClient::ping`/`LocalClient::ping` used to send it as a `tools/call` and would
//! have failed against a real server.

mod support;

use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::local::LocalClient;
use ctrader_mcp::remote::RemoteClient;
use ctrader_mcp::transport::McpSession;
use support::MockMcpServer;

/// `rmcp`'s default `ServerHandler::ping` implementation (which `MockMcpServer` doesn't
/// override) already answers `Ok(())`: no tool registration needed for this to work.
#[tokio::test]
async fn session_ping_succeeds_without_registering_a_ping_tool() {
    let server = MockMcpServer::builder().build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;

    let session = McpSession::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed");

    session
        .ping()
        .await
        .expect("a protocol-level ping must succeed even with zero tools registered");

    handle.abort();
}

#[tokio::test]
async fn remote_client_ping_succeeds_without_a_ping_tool() {
    let server = MockMcpServer::builder().build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;

    let client = RemoteClient::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed");

    client
        .ping()
        .await
        .expect("RemoteClient::ping must use the protocol-level ping, not a tools/call");

    handle.abort();
}

#[tokio::test]
async fn local_client_ping_succeeds_without_a_ping_tool() {
    let server = MockMcpServer::builder().build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;

    let client = LocalClient::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed");

    client
        .ping()
        .await
        .expect("LocalClient::ping must use the protocol-level ping, not a tools/call");

    handle.abort();
}
