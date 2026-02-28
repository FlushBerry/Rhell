pub const BUF_SIZE: usize = 4096;
pub const RECONNECT_DELAY_MS: u64 = 5000;
pub const MAX_RETRIES: u32 = 100;
pub const END_MARKER: &[u8] = b"\n<E>\n";
pub const HANDSHAKE_REQ: &[u8] = b"RS_H1";
pub const HANDSHAKE_ACK: &[u8] = b"RS_A1";

// Marqueurs transfert fichier
pub const FILE_BEGIN: &[u8] = b"<FILE_BEGIN>";
pub const FILE_END: &[u8] = b"<FILE_END>";
pub const FILE_ERR: &[u8] = b"<FILE_ERR>";
pub const FILE_OK: &[u8] = b"<FILE_OK>";

pub fn parse_args() -> (String, u16, Option<String>) {
    let args: Vec<String> = std::env::args().collect();
    let mut host = String::from("127.0.0.1");
    let mut port: u16 = 4444;
    let mut extra: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-H" | "--host" => {
                if i + 1 < args.len() {
                    host = args[i + 1].clone();
                    i += 1;
                }
            }
            "-p" | "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(4444);
                    i += 1;
                }
            }
            "-s" | "--shell" | "-l" | "--log" => {
                if i + 1 < args.len() {
                    extra = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    (host, port, extra)
}

pub fn contains_end_marker(buf: &[u8]) -> bool {
    if buf.len() < END_MARKER.len() {
        return false;
    }
    buf.windows(END_MARKER.len()).any(|w| w == END_MARKER)
}

pub fn strip_end_marker(data: &[u8]) -> Vec<u8> {
    let ml = END_MARKER.len();
    if data.len() < ml {
        return data.to_vec();
    }
    for i in (0..=data.len() - ml).rev() {
        if &data[i..i + ml] == END_MARKER {
            let mut r = Vec::with_capacity(data.len() - ml);
            r.extend_from_slice(&data[..i]);
            r.extend_from_slice(&data[i + ml..]);
            return r;
        }
    }
    data.to_vec()
}

pub fn contains_marker(buf: &[u8], marker: &[u8]) -> bool {
    if buf.len() < marker.len() {
        return false;
    }
    buf.windows(marker.len()).any(|w| w == marker)
}

/// Lire N octets exact depuis un stream
pub fn read_exact_bytes(stream: &mut dyn std::io::Read, n: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; n];
    let mut pos = 0;
    while pos < n {
        let r = stream.read(&mut buf[pos..]).map_err(|e| e.to_string())?;
        if r == 0 {
            return Err("connection closed".into());
        }
        pos += r;
    }
    Ok(buf)
}

/// Envoyer une taille en 8 octets big-endian
pub fn send_size(stream: &mut dyn std::io::Write, size: u64) -> Result<(), String> {
    stream
        .write_all(&size.to_be_bytes())
        .map_err(|e| e.to_string())
}

/// Recevoir une taille en 8 octets big-endian
pub fn recv_size(stream: &mut dyn std::io::Read) -> Result<u64, String> {
    let buf = read_exact_bytes(stream, 8)?;
    Ok(u64::from_be_bytes([
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
    ]))
}
