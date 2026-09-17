use reposlice_workspace::{install_local_crash_reporting, load_cached_workspace_model};
use serde_json::{json, Value};
use std::env;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

const MCP_PROTOCOL_VERSION: &str = "2026-07-28";
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_local_crash_reporting();
    let bind = option_value("--bind").unwrap_or_else(|| "127.0.0.1:8765".to_string());
    let address: SocketAddr = bind.parse()?;
    if !address.ip().is_loopback() {
        return Err("RepoSlice MCP refuses non-loopback addresses; use 127.0.0.1 or ::1".into());
    }
    let listener = TcpListener::bind(address)?;
    eprintln!("RepoSlice MCP {MCP_PROTOCOL_VERSION} listening on http://{address}/mcp");
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
                stream.set_write_timeout(Some(Duration::from_secs(15))).ok();
                if let Err(error) = handle_connection(&mut stream) {
                    eprintln!("RepoSlice MCP request failed: {error}");
                }
            }
            Err(error) => eprintln!("RepoSlice MCP connection failed: {error}"),
        }
    }
    Ok(())
}

fn handle_connection(stream: &mut TcpStream) -> io::Result<()> {
    if !stream.peer_addr()?.ip().is_loopback() {
        return write_http(stream, 403, &json!({"error":"loopback-only"}).to_string());
    }
    let request = read_http_request(stream)?;
    if request.method != "POST" || request.path != "/mcp" {
        return write_http(stream, 404, &json!({"error":"not found"}).to_string());
    }
    if request.header("transfer-encoding").is_some() {
        return write_http(
            stream,
            400,
            &json!({"error":"transfer encoding is not supported"}).to_string(),
        );
    }
    if request.header("content-length").is_none() {
        return write_http(
            stream,
            400,
            &json!({"error":"content-length is required"}).to_string(),
        );
    }
    if !request
        .header("content-type")
        .is_some_and(is_json_content_type)
    {
        return write_http(
            stream,
            400,
            &json!({"error":"content-type must be application/json"}).to_string(),
        );
    }
    let body: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(error) => {
            return write_json_rpc_error(
                stream,
                Value::Null,
                -32700,
                &format!("Parse error: {error}"),
            )
        }
    };
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    if request.header("mcp-protocol-version") != Some(MCP_PROTOCOL_VERSION) {
        return write_json_rpc_error(
            stream,
            id,
            -32600,
            "Unsupported or missing MCP-Protocol-Version",
        );
    }
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    if request.header("mcp-method") != Some(method) {
        return write_json_rpc_error(
            stream,
            id,
            -32600,
            "Mcp-Method header does not match request method",
        );
    }
    if method == "tools/call" {
        let name = body
            .pointer("/params/name")
            .and_then(Value::as_str)
            .unwrap_or("");
        if request.header("mcp-name") != Some(name) {
            return write_json_rpc_error(
                stream,
                id,
                -32600,
                "Mcp-Name header does not match tool name",
            );
        }
    }

    let result = match method {
        "server/discover" => Ok(discover_result()),
        "tools/list" => Ok(list_tools_result()),
        "tools/call" => call_tool(body.get("params").unwrap_or(&Value::Null)),
        _ => Err((-32601, format!("Method not found: {method}"))),
    };
    match result {
        Ok(result) => write_http(
            stream,
            200,
            &json!({"jsonrpc":"2.0","id":id,"result":result}).to_string(),
        ),
        Err((code, message)) => write_json_rpc_error(stream, id, code, &message),
    }
}

fn discover_result() -> Value {
    json!({
        "resultType":"complete",
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": {"tools":{}},
        "serverInfo": {"name":"RepoSlice","version":env!("CARGO_PKG_VERSION")},
        "ttlMs": 30_000,
        "cacheScope":"private",
        "_meta":{"io.modelcontextprotocol/serverInfo":{"name":"RepoSlice","version":env!("CARGO_PKG_VERSION")}}
    })
}

fn list_tools_result() -> Value {
    json!({
        "resultType":"complete",
        "ttlMs":30_000,
        "cacheScope":"private",
        "tools":[
            tool("workspace_status", "Return the current cached RepoSlice workspace summary", json!({"type":"object","properties":{"workspaceId":{"type":"string"}},"required":["workspaceId"],"additionalProperties":false}))
        ],
        "_meta":{"io.modelcontextprotocol/serverInfo":{"name":"RepoSlice","version":env!("CARGO_PKG_VERSION")}}
    })
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name":name,"description":description,"inputSchema":input_schema})
}

fn call_tool(params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "Tool name is required".to_string()))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let output = match name {
        "workspace_status" => workspace_status(&arguments),
        _ => return Err((-32602, format!("Unknown tool: {name}"))),
    };
    match output {
        Ok(structured) => {
            let text = serde_json::to_string_pretty(&structured)
                .unwrap_or_else(|_| structured.to_string());
            Ok(json!({
                "resultType":"complete",
                "content":[{"type":"text","text":text}],
                "structuredContent":structured,
                "isError":false,
                "_meta":{"io.modelcontextprotocol/serverInfo":{"name":"RepoSlice","version":env!("CARGO_PKG_VERSION")}}
            }))
        }
        Err(message) => Ok(json!({
            "resultType":"complete",
            "content":[{"type":"text","text":message}],
            "isError":true,
            "_meta":{"io.modelcontextprotocol/serverInfo":{"name":"RepoSlice","version":env!("CARGO_PKG_VERSION")}}
        })),
    }
}

fn workspace_status(arguments: &Value) -> Result<Value, String> {
    let workspace_id = required_string(arguments, "workspaceId")?;
    let model = current_workspace(workspace_id)?;
    let project_units = model
        .repositories
        .iter()
        .map(|repository| repository.project_units.len())
        .sum::<usize>();
    let components = model
        .repositories
        .iter()
        .flat_map(|repository| &repository.project_units)
        .map(|unit| unit.model.components.len())
        .sum::<usize>();
    let entrypoints = model
        .repositories
        .iter()
        .flat_map(|repository| &repository.project_units)
        .map(|unit| unit.model.entrypoints.len())
        .sum::<usize>();
    let dependencies = model
        .repositories
        .iter()
        .flat_map(|repository| &repository.project_units)
        .map(|unit| unit.model.dependencies.len())
        .sum::<usize>();
    Ok(json!({
        "workspaceId": model.id,
        "workspaceName": model.name,
        "repositories": model.repositories.len(),
        "projectUnits": project_units,
        "components": components,
        "entrypoints": entrypoints,
        "dependencies": dependencies,
        "crossProjectDependencies": model.cross_project_dependencies.len()
    }))
}

fn current_workspace(workspace_id: &str) -> Result<reposlice_core::WorkspaceModel, String> {
    load_cached_workspace_model(workspace_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            "No valid cached workspace model is available; run an analysis first".to_string()
        })
}

fn required_string<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Argument '{name}' is required"))
}

struct HttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

fn read_http_request(stream: &mut TcpStream) -> io::Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request too large",
            ));
        }
        if let Some(index) = find_subslice(&buffer, b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }
    let head = std::str::from_utf8(&buffer[..header_end])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP headers"))?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let path = request_parts.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();
    let mut content_length = 0usize;
    for line in lines.filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let key = name.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        if key == "content-length" {
            content_length = value.parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid content-length")
            })?;
        }
        headers.push((key, value));
    }
    if content_length > MAX_REQUEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request body too large",
        ));
    }
    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete request body",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_REQUEST_BYTES + header_end {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request too large",
            ));
        }
    }
    Ok(HttpRequest {
        method,
        path,
        headers,
        body: buffer[header_end..header_end + content_length].to_vec(),
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn is_json_content_type(value: &str) -> bool {
    value
        .split(';')
        .next()
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
}

fn write_json_rpc_error(
    stream: &mut TcpStream,
    id: Value,
    code: i64,
    message: &str,
) -> io::Result<()> {
    write_http(
        stream,
        200,
        &json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}).to_string(),
    )
}

fn write_http(stream: &mut TcpStream, status: u16, body: &str) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let response = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\n\r\n{body}", body.len());
    stream.write_all(response.as_bytes())
}

fn option_value(name: &str) -> Option<String> {
    let arguments = env::args().collect::<Vec<_>>();
    arguments
        .iter()
        .position(|argument| argument == name)
        .and_then(|index| arguments.get(index + 1))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_catalog_is_cacheable_and_modern() {
        let result = list_tools_result();
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["cacheScope"], "private");
        let tools = result["tools"]
            .as_array()
            .expect("tool catalog must list tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "workspace_status");
    }

    #[test]
    fn only_loopback_is_allowed_by_design() {
        assert!(std::net::IpAddr::from([127, 0, 0, 1]).is_loopback());
        assert!(!std::net::IpAddr::from([0, 0, 0, 0]).is_loopback());
    }

    #[test]
    fn json_content_type_accepts_charset_but_not_other_media_types() {
        assert!(is_json_content_type("application/json"));
        assert!(is_json_content_type("application/json; charset=utf-8"));
        assert!(!is_json_content_type("text/plain"));
    }
}
