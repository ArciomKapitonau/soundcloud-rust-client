//! Optional local Discord IPC presence. No Discord token or network API is used.
use crate::audio::{Player, PlayerState, Status};
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn write_frame(stream: &mut impl Write, opcode: u32, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value)?;
    stream.write_all(&opcode.to_le_bytes())?;
    stream.write_all(&(body.len() as u32).to_le_bytes())?;
    stream.write_all(&body)
}

fn read_frame(stream: &mut impl Read) -> io::Result<(u32, Value)> {
    let mut header = [0; 8];
    stream.read_exact(&mut header)?;
    let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
    let len = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
    if len > 65536 { return Err(io::Error::other("oversized Discord frame")); }
    let mut body = vec![0; len];
    stream.read_exact(&mut body)?;
    Ok((opcode, serde_json::from_slice(&body)?))
}

fn response(stream: &mut UnixStream) -> io::Result<Value> {
    for _ in 0..16 {
        let (op, value) = read_frame(stream)?;
        match op {
            1 if value["evt"] != "ERROR" => return Ok(value),
            3 => write_frame(stream, 4, &value)?,
            _ => return Err(io::Error::other("Discord rejected RPC session")),
        }
    }
    Err(io::Error::other("too many Discord ping frames"))
}

fn connect(id: &str) -> io::Result<UnixStream> {
    let mut roots: Vec<PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
    roots.push(PathBuf::from("/tmp"));
    for root in roots {
        for sub in ["", "app/com.discordapp.Discord", "snap.discord"] {
            for n in 0..10 {
                if let Ok(mut stream) = UnixStream::connect(root.join(sub).join(format!("discord-ipc-{n}"))) {
                    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                    write_frame(&mut stream, 0, &json!({"v":1,"client_id":id}))?;
                    let ready = response(&mut stream)?;
                    if ready["evt"] == "READY" { return Ok(stream); }
                }
            }
        }
    }
    Err(io::Error::other("Discord not available"))
}

fn text(value: &str) -> String {
    let mut s: String = value.chars().take(128).collect();
    if s.chars().count() < 2 { s.push_str("  "); }
    s
}

fn activity(state: &PlayerState, now: u64) -> Value {
    let Some(track) = &state.track else { return Value::Null };
    if !matches!(state.status, Status::Playing | Status::Paused) { return Value::Null; }
    let mut result = json!({"type":2,"details":text(&track.title),"state":text(&track.artist)});
    if state.status == Status::Playing {
        let pos = state.position.max(0.0).min(state.duration.max(0.0)) as u64;
        result["timestamps"] = json!({"start":now.saturating_sub(pos),"end":now.saturating_add((state.duration.max(0.0) as u64).saturating_sub(pos))});
    } else {
        result["state"] = json!(text(&format!("Пауза · {}", track.artist)));
    }
    if let Some(url) = &track.artwork_url {
        if url.starts_with("https://") { result["assets"] = json!({"large_image":url,"large_text":text(&track.title)}); }
    }
    result
}

pub fn start(player: &Arc<Player>) {
    let Ok(id) = std::env::var("DISCORD_APPLICATION_ID") else { return };
    if id.parse::<u64>().ok().filter(|v| *v > 0).is_none() {
        log::warn!("DISCORD_APPLICATION_ID must be a numeric application ID");
        return;
    }
    let player = Arc::downgrade(player);
    if let Err(e) = std::thread::Builder::new().name("discord-rpc".into()).spawn(move || run(player, id)) {
        log::warn!("cannot start Discord RPC: {e}");
    }
}

fn run(player: Weak<Player>, id: String) {
    let mut connection = None;
    let mut nonce = 0u64;
    loop {
        let Some(player) = player.upgrade() else { break };
        let state = player.state();
        drop(player);
        if connection.is_none() { connection = connect(&id).ok(); }
        if let Some(stream) = &mut connection {
            nonce = nonce.wrapping_add(1);
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let request = json!({"cmd":"SET_ACTIVITY","args":{"pid":std::process::id(),"activity":activity(&state,now)},"nonce":nonce.to_string()});
            if write_frame(stream, 1, &request).and_then(|_| response(stream)).is_err() { connection = None; }
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playback_pause_stop_and_seek_payloads() {
        let track = crate::models::Track::from_value(&json!({"id":1,"title":"Track","duration":120000,"user":{"username":"Artist"}})).unwrap();
        let mut state = PlayerState { track:Some(track), index:Some(0), queue_len:1,
            status:Status::Playing, position:30.0, duration:120.0, volume:1.0,
            repeat:crate::config::RepeatMode::Off, liked:false, preview:false,
            error:None, output_device:None };
        let playing = activity(&state, 1000);
        assert_eq!(playing["timestamps"]["start"], 970);
        assert_eq!(playing["timestamps"]["end"], 1090);
        state.position = 60.0;
        assert_eq!(activity(&state, 1000)["timestamps"]["end"], 1060);
        state.status = Status::Paused;
        assert!(activity(&state, 1000).get("timestamps").is_none());
        state.status = Status::Idle;
        assert!(activity(&state, 1000).is_null());
        state.status = Status::Loading;
        assert!(activity(&state, 1000).is_null());
    }

    #[test]
    fn framing_roundtrip_and_size_limit() {
        let value = json!({"details":"Трек 🎵", "activity":null});
        let mut bytes = Vec::new();
        write_frame(&mut bytes, 1, &value).unwrap();
        assert_eq!(read_frame(&mut bytes.as_slice()).unwrap(), (1, value));
        let mut oversized = Vec::from(1u32.to_le_bytes());
        oversized.extend_from_slice(&65537u32.to_le_bytes());
        assert!(read_frame(&mut oversized.as_slice()).is_err());
        assert!(read_frame(&mut &bytes[..5]).is_err());
    }
}
