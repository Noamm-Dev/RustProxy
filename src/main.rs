mod config;
mod packet_utils;

use std::io::{Error, ErrorKind, Result};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::io::{copy_bidirectional, AsyncWriteExt};
use tokio::net::{lookup_host, TcpListener, TcpSocket, TcpStream};
use tokio::time::timeout;

use config::{CONFIG, ICON};
use packet_utils::read_raw_packet;

const PORT: u16 = 25565;
const PING_TIMEOUT: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind(("0.0.0.0", PORT)).await
        .unwrap_or_else(|e| panic!("Failed to bind 0.0.0.0:{PORT}: {e}"));
   
    LazyLock::force(&ICON);
    println!("🚀 Proxy active! {:?}", *CONFIG);

    loop {
        let Ok((client, _)) = listener.accept().await else {
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        };

        tokio::spawn(async move {
            if let Err(e) = handle_client(client).await {
                eprintln!("{e}");
            }
        });
    }
}

async fn handle_client(mut client: TcpStream) -> Result<()> {
    let Ok(handshake_raw) = read_raw_packet(&mut client).await else { return Ok(()) };
    let next_state = packet_utils::get_next_state(&handshake_raw).ok_or_else(|| invalid("Malformed handshake"))?;

    if next_state == 1 {
        read_raw_packet(&mut client).await?;
        let json = json!({
            "version": { "name": CONFIG.version, "protocol": CONFIG.protocol },
            "players": { "max": CONFIG.max_players, "online": CONFIG.online_players },
            "description": {
                "text": format!("{}\nServer: {} | Ping: {}ms", CONFIG.desc, CONFIG.server, get_latency(&CONFIG.server).await)
            },
            "favicon": *ICON,
        });

        let mut packet = packet_utils::write_var_int(0x00);
        packet.extend(packet_utils::write_string(&json.to_string()));
        client.write_all(&packet_utils::frame(packet)).await?;
        return Ok(());
    }

    let rewritten = packet_utils::rewrite_handshake_hostname(&handshake_raw, &CONFIG.server)
        .ok_or_else(|| invalid("Malformed handshake"))?;

    let mut target = connect(&CONFIG.server).await?;
    target.write_all(&rewritten).await?;

    let Ok(login_start) = read_raw_packet(&mut client).await else { return Ok(()) };

    let mut input = login_start.as_slice();
    let username = packet_utils::read_var_int_from(&mut input)
        .and_then(|_| packet_utils::read_var_int_from(&mut input))
        .and_then(|_| packet_utils::read_string_from(&mut input));
    let Some(username) = username else { return Ok(()) };

    if !CONFIG.whitelist.contains(&username) {
        client.write_all(&packet_utils::create_disconnect_packet("§cYou are not whitelisted!")).await?;
        return Ok(());
    }

    target.write_all(&login_start).await?;

    println!("➡️ {username} joined");
    let start = Instant::now();
    let _ = copy_bidirectional(&mut client, &mut target).await;
    println!("⬅️ {username} left after {}", format_duration(start.elapsed()));
    Ok(())
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

async fn connect(host: &str) -> Result<TcpStream> {
    let addr = lookup_host((host, PORT)).await?
        .next()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("No address for {host}")))?;

    let socket = if addr.is_ipv4() { TcpSocket::new_v4()? } else { TcpSocket::new_v6()? };
    socket.set_keepalive(true)?;
    socket.set_nodelay(true)?;
    socket.connect(addr).await
}

async fn get_latency(host: &str) -> i64 {
    let Some(addr) = lookup_host((host, PORT)).await.ok().and_then(|mut addrs| addrs.next()) else { return -1 };

    let start = Instant::now();
    match timeout(PING_TIMEOUT, TcpStream::connect(addr)).await {
        Ok(Ok(_)) => start.elapsed().as_millis() as i64,
        _ => -1,
    }
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
