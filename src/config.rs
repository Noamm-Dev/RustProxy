use std::collections::HashSet;
use std::fs;
use std::sync::LazyLock;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Deserialize;

const PROXY_FILE: &str = "/apps/files/proxy.json";
const MOTD_ICON: &str = "/apps/files/server.png";

pub static CONFIG: LazyLock<ProxyConfig> = LazyLock::new(|| {
    let raw = fs::read_to_string(PROXY_FILE).unwrap_or_else(|e| panic!("Failed to read {PROXY_FILE}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("Invalid {PROXY_FILE}: {e}"))
});

pub static ICON: LazyLock<String> = LazyLock::new(|| {
    let bytes = fs::read(MOTD_ICON).unwrap_or_else(|e| panic!("Failed to read {MOTD_ICON}: {e}"));
    format!("data:image/png;base64,{}", STANDARD.encode(bytes))
});

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub server: String,
    pub version: String,
    pub protocol: i32,
    pub desc: String,
    pub online_players: i32,
    pub max_players: i32,
    pub whitelist: HashSet<String>,
}
