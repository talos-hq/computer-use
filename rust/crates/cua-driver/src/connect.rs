//! `connect`: registering the driver as an MCP server in local coding agents.
//!
//! Upstream implements this with `cua-agent-setup`, which lives outside the
//! cua-driver tree and embeds the whole cua skills directory. puppet does not
//! carry it yet; the command reports that and exits non-zero.

pub fn run_connect(
    _clients: &[String],
    _tools: &serde_json::Value,
    _dry_run: bool,
    _replace: bool,
    json: bool,
) {
    let message = "connect is not available in puppet yet; add the MCP server to your agent manually (command: `<binary> mcp`)";
    if json {
        println!("{}", serde_json::json!({ "ok": false, "error": message }));
    } else {
        eprintln!("{message}");
    }
    std::process::exit(2);
}
