pub mod transport;
pub mod types;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::Client;

use transport::send_request;
use types::{ContentBlock, JsonRpcRequest, JsonRpcResponse, ToolResult};

#[allow(unused_imports)]
use types::ToolsList;

const MCP_ENDPOINT: &str = "https://mcp.deepwiki.com/mcp";

pub struct McpClient {
    client: Client,
    endpoint: String,
    session_id: Option<String>,
    request_id: AtomicU64,
    timeout: Duration,
}

impl McpClient {
    /// Create a new MCP client and perform the initialization handshake.
    pub async fn connect(timeout: Duration) -> Result<Self> {
        let client = Client::builder()
            .build()
            .context("Failed to build HTTP client")?;

        let mut mcp = Self {
            client,
            endpoint: MCP_ENDPOINT.to_string(),
            session_id: None,
            request_id: AtomicU64::new(1),
            timeout,
        };

        mcp.initialize().await?;
        Ok(mcp)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn with_endpoint(endpoint: String, timeout: Duration) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            session_id: None,
            request_id: AtomicU64::new(1),
            timeout,
        }
    }

    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Perform the MCP initialization handshake.
    async fn initialize(&mut self) -> Result<()> {
        let init_request = JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            serde_json::json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {
                    "name": "dw2md",
                    "version": "0.2.0"
                }
            }),
        );

        let response = send_request(
            &self.client,
            &self.endpoint,
            &init_request,
            None,
            self.timeout,
        )
        .await
        .context("MCP initialization failed — the DeepWiki MCP endpoint may be down")?;

        if let Some(sid) = response.session_id {
            self.session_id = Some(sid);
        }

        // Send initialized notification
        let notification =
            JsonRpcRequest::notification("notifications/initialized", serde_json::json!({}));

        // Notification response is best-effort — ignore errors
        let _ = send_request(
            &self.client,
            &self.endpoint,
            &notification,
            self.session_id.as_deref(),
            self.timeout,
        )
        .await;

        Ok(())
    }

    /// Call `tools/list` to discover available tool schemas.
    #[allow(dead_code)]
    pub async fn list_tools(&self) -> Result<ToolsList> {
        let request = JsonRpcRequest::new(self.next_id(), "tools/list", serde_json::json!({}));

        let resp = self.send(&request).await?;
        let rpc: JsonRpcResponse =
            serde_json::from_value(resp).context("Failed to parse tools/list response")?;

        if let Some(err) = rpc.error {
            bail!("tools/list failed: {}", err);
        }

        let result = rpc.result.context("tools/list returned no result")?;
        let tools: ToolsList =
            serde_json::from_value(result).context("Failed to parse tools list")?;
        Ok(tools)
    }

    /// Call an MCP tool and return the text content.
    pub async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<String> {
        let request = JsonRpcRequest::new(
            self.next_id(),
            "tools/call",
            serde_json::json!({
                "name": name,
                "arguments": arguments,
            }),
        );

        let resp = self.send(&request).await?;
        let rpc: JsonRpcResponse =
            serde_json::from_value(resp).context("Failed to parse tool call response")?;

        if let Some(err) = rpc.error {
            bail!("Tool '{}' returned error: {}", name, err);
        }

        let result = rpc.result.context("Tool call returned no result")?;
        let tool_result: ToolResult =
            serde_json::from_value(result).context("Failed to parse tool result")?;

        if tool_result.is_error {
            let error_text = extract_text(&tool_result.content);
            bail!("Tool '{}' reported error: {}", name, error_text);
        }

        Ok(extract_text(&tool_result.content))
    }

    /// Send a JSON-RPC request and validate the response id matches.
    async fn send(&self, request: &JsonRpcRequest) -> Result<serde_json::Value> {
        let expected_id = request.id;

        let response = send_request(
            &self.client,
            &self.endpoint,
            request,
            self.session_id.as_deref(),
            self.timeout,
        )
        .await?;

        // Validate response id matches request id (skip for notifications which have no id)
        if let Some(expected) = expected_id {
            let rpc: JsonRpcResponse = serde_json::from_value(response.body.clone())
                .context("Failed to parse JSON-RPC response for id validation")?;
            match rpc.id {
                Some(actual) if actual != expected => {
                    bail!(
                        "JSON-RPC id mismatch: expected {}, got {}",
                        expected,
                        actual
                    );
                }
                None => {
                    bail!("JSON-RPC response missing id (expected {})", expected);
                }
                _ => {}
            }
        }

        Ok(response.body)
    }
}

/// Extract concatenated text from content blocks.
fn extract_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .map(|b| match b {
            ContentBlock::Text { text } => text.as_str(),
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_single() {
        let blocks = vec![ContentBlock::Text {
            text: "hello world".to_string(),
        }];
        assert_eq!(extract_text(&blocks), "hello world");
    }

    #[test]
    fn test_extract_text_multiple() {
        let blocks = vec![
            ContentBlock::Text {
                text: "part 1".to_string(),
            },
            ContentBlock::Text {
                text: "part 2".to_string(),
            },
        ];
        assert_eq!(extract_text(&blocks), "part 1part 2");
    }

    #[test]
    fn test_extract_text_empty() {
        let blocks: Vec<ContentBlock> = vec![];
        assert_eq!(extract_text(&blocks), "");
    }
}
