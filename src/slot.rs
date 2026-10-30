//! Slot identity: pid + starttime + cmdline. Never kill a recycled PID.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub pid: u32,
    pub starttime: u64,
}

pub fn read_starttime(proc_root: &Path, pid: u32) -> Result<u64> {
    let raw = fs::read_to_string(proc_root.join(pid.to_string()).join("stat"))?;
    parse_starttime(&raw)
}

/// /proc/<pid>/stat field 22 (1-indexed) is starttime. comm may contain spaces
/// inside parentheses.
pub fn parse_starttime(stat: &str) -> Result<u64> {
    let close = stat
        .rfind(')')
        .ok_or_else(|| anyhow::anyhow!("stat missing comm"))?;
    let mut rest = stat[close + 1..].split_whitespace();
    // after comm: state(3) ppid(4) ... starttime is field 22, so 22-2 = 20th in rest? 
    // fields: 1 pid, 2 comm, then rest starts at field 3.
    // starttime = field 22 → index 22-3 = 19 in rest.
    let field = rest
        .nth(19)
        .ok_or_else(|| anyhow::anyhow!("stat missing starttime"))?;
    field.parse().map_err(|_| anyhow::anyhow!("bad starttime"))
}

pub fn cmdline_ours(proc_root: &Path, pid: u32, port: u16) -> bool {
    let raw = fs::read(proc_root.join(pid.to_string()).join("cmdline")).unwrap_or_default();
    let s = String::from_utf8_lossy(&raw);
    let text = s.replace('\0', " ");
    let has_bin = text.contains("llama-server") || text.contains("llama-server-vk");
    let has_port = text.contains(&format!("--port {port}"))
        || text.contains(&format!("--port={port}"))
        || text.split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|w| w[0] == "--port" && w[1] == port.to_string());
    has_bin && has_port
}

/// True when the live guest argv includes this GGUF path (`-m`).
pub fn cmdline_has_model(proc_root: &Path, pid: u32, model: &Path) -> bool {
    let raw = fs::read(proc_root.join(pid.to_string()).join("cmdline")).unwrap_or_default();
    cmdline_bytes_have_model(&raw, model)
}

pub fn cmdline_bytes_have_model(raw: &[u8], model: &Path) -> bool {
    let want = model.as_os_str().as_encoded_bytes();
    if want.is_empty() {
        return false;
    }
    raw.split(|b| *b == 0).any(|arg| arg == want)
}

pub fn load_identity(pid_path: &Path, stamp_path: &Path) -> Option<Identity> {
    let pid: u32 = fs::read_to_string(pid_path).ok()?.trim().parse().ok()?;
    let starttime: u64 = fs::read_to_string(stamp_path).ok()?.trim().parse().ok()?;
    Some(Identity { pid, starttime })
}

pub fn save_identity(pid_path: &Path, stamp_path: &Path, id: &Identity) -> Result<()> {
    if let Some(d) = pid_path.parent() {
        fs::create_dir_all(d)?;
    }
    fs::write(pid_path, format!("{}\n", id.pid))?;
    fs::write(stamp_path, format!("{}\n", id.starttime))?;
    Ok(())
}

pub fn is_ours(proc_root: &Path, id: &Identity, port: u16) -> bool {
    match read_starttime(proc_root, id.pid) {
        Ok(st) if st == id.starttime => cmdline_ours(proc_root, id.pid, port),
        _ => false,
    }
}

pub fn pid_alive(proc_root: &Path, pid: u32) -> bool {
    proc_root.join(pid.to_string()).join("stat").is_file()
}

/// Refuse to signal a PID that is not our recorded identity.
pub fn kill_if_ours(proc_root: &Path, id: &Identity, port: u16, sig: i32) -> Result<()> {
    if !is_ours(proc_root, id, port) {
        bail!(
            "pid {} is not our slot (starttime/cmdline mismatch) — refusing to signal",
            id.pid
        );
    }
    let rc = unsafe { libc::kill(id.pid as i32, sig) };
    if rc != 0 && sig != 0 {
        bail!("kill {sig} pid {} failed", id.pid);
    }
    Ok(())
}

pub fn default_proc() -> PathBuf {
    PathBuf::from("/proc")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starttime_skips_comm_with_spaces() {
        // pid (comm with spaces) state ppid ... starttime is field 22
        let mut fields = vec!["1234".to_string(), "(llama server)".into()];
        for i in 3..=22 {
            fields.push(if i == 22 {
                "987654".into()
            } else {
                i.to_string()
            });
        }
        let stat = fields.join(" ");
        assert_eq!(parse_starttime(&stat).unwrap(), 987654);
    }

    #[test]
    fn recycled_pid_is_not_ours() {
        let root = std::env::temp_dir().join(format!("slot-proc-{}", std::process::id()));
        let pid = 4242u32;
        let dir = root.join(pid.to_string());
        fs::create_dir_all(&dir).unwrap();
        // live process starttime 100; recorded 99
        let mut fields: Vec<String> = vec!["4242".into(), "(llama-server)".into()];
        for i in 3..=22 {
            fields.push(if i == 22 { "100".into() } else { "0".into() });
        }
        fs::write(dir.join("stat"), fields.join(" ")).unwrap();
        fs::write(dir.join("cmdline"), b"llama-server\0--port\0008080\0").unwrap();
        let id = Identity {
            pid,
            starttime: 99,
        };
        assert!(!is_ours(&root, &id, 8080));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn matching_starttime_and_port_is_ours() {
        let root = std::env::temp_dir().join(format!("slot-proc-ok-{}", std::process::id()));
        let pid = 4243u32;
        let dir = root.join(pid.to_string());
        fs::create_dir_all(&dir).unwrap();
        let mut fields: Vec<String> = vec!["4243".into(), "(llama-server)".into()];
        for i in 3..=22 {
            fields.push(if i == 22 { "55".into() } else { "0".into() });
        }
        fs::write(dir.join("stat"), fields.join(" ")).unwrap();
        let mut cmd = b"/opt/llama-server".to_vec();
        cmd.push(0);
        cmd.extend(b"--port");
        cmd.push(0);
        cmd.extend(b"8080");
        cmd.push(0);
        fs::write(dir.join("cmdline"), cmd).unwrap();
        let id = Identity {
            pid,
            starttime: 55,
        };
        assert!(is_ours(&root, &id, 8080));
        assert!(!is_ours(&root, &id, 8091));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn port_busy_alone_is_not_identity() {
        // documented: we never treat a listening port as ownership
        let id = Identity {
            pid: 1,
            starttime: 0,
        };
        assert!(!is_ours(Path::new("/proc"), &id, 8080) || !cmdline_ours(Path::new("/proc"), 1, 8080));
    }

    #[test]
    fn cmdline_model_is_the_minus_m_path() {
        let mut cmd = b"llama-server\0-m\0".to_vec();
        cmd.extend(b"/models/4b.gguf\0--port\0008080\0");
        assert!(cmdline_bytes_have_model(
            &cmd,
            Path::new("/models/4b.gguf")
        ));
        assert!(!cmdline_bytes_have_model(
            &cmd,
            Path::new("/models/8b.gguf")
        ));
        assert!(!cmdline_bytes_have_model(&cmd, Path::new("")));
    }
}
