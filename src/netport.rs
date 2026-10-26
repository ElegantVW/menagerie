//! Is anyone listening on a TCP port (IPv4). Not ownership.

use std::fs;

pub fn listening(port: u16) -> bool {
    listening_in("/proc/net/tcp", port)
}

/// /proc/net/tcp: local_address is hex IP:port, state 0A = LISTEN.
pub fn listening_in(proc_tcp: &str, port: u16) -> bool {
    let Ok(s) = fs::read_to_string(proc_tcp) else {
        return false;
    };
    let want = format!(":{port:04X}");
    for line in s.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        // local_address, rem, state
        if cols.get(3) != Some(&"0A") {
            continue;
        }
        if cols[1].to_ascii_uppercase().ends_with(&want) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_listen_hex() {
        let sample = "  sl  local_address rem_address   st\n   0: 0100007F:1F90 00000000:0000 0A\n";
        let dir = std::env::temp_dir().join(format!("tcp-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("tcp");
        std::fs::write(&p, sample).unwrap();
        assert!(listening_in(p.to_str().unwrap(), 8080));
        assert!(!listening_in(p.to_str().unwrap(), 8091));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
