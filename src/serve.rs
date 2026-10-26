//! ensure / start / stop / status. Guest llama-server only.

use crate::backend::{self, SpawnSpec};
use crate::config::{AppEntry, Registry};
use crate::paths;
use crate::slot::{self, Identity};
use crate::vram;
use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const HOST: &str = "127.0.0.1";

pub fn threads() -> u32 {
    env_u32("PIXIE_LLM_THREADS", 8)
}
pub fn threads_batch() -> u32 {
    env_u32("PIXIE_LLM_THREADS_BATCH", 16)
}
fn env_u32(k: &str, d: u32) -> u32 {
    std::env::var(k)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(d)
}

pub struct Mouth {
    pub app: String,
    pub entry: AppEntry,
    pub model_path: std::path::PathBuf,
    pub model_name: String,
    pub vram_gb: Option<f64>,
    pub size: i64,
}

impl Mouth {
    pub fn from_reg(reg: &Registry, name: &str) -> Result<Self> {
        let (app, entry) = reg.resolve_app(name)?;
        let model = reg
            .models
            .get(&entry.model)
            .with_context(|| format!("model '{}' not in registry", entry.model))?;
        Ok(Self {
            app: app.to_string(),
            entry: entry.clone(),
            model_path: Path::new(&model.path).to_path_buf(),
            model_name: entry.model.clone(),
            vram_gb: model.vram_gb,
            size: model.size,
        })
    }

    pub fn url(&self) -> String {
        format!("http://{HOST}:{}", self.entry.port)
    }
}

pub fn health(port: u16) -> Option<u16> {
    let addr = format!("{HOST}:{port}");
    let mut s = TcpStream::connect_timeout(
        &addr.parse().ok()?,
        Duration::from_secs(2),
    )
    .ok()?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let req = format!("GET /health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes()).ok()?;
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    if buf.contains("200") {
        Some(200)
    } else if buf.contains("503") {
        Some(503)
    } else {
        None
    }
}

pub fn ours_running(mouth: &Mouth) -> bool {
    let Some(id) = slot::load_identity(&paths::pid_file(&mouth.app), &paths::stamp_file(&mouth.app))
    else {
        return false;
    };
    slot::is_ours(&slot::default_proc(), &id, mouth.entry.port)
}

pub fn port_held_by_stranger(mouth: &Mouth) -> bool {
    crate::netport::listening(mouth.entry.port) && !ours_running(mouth)
}

pub fn ensure(reg: &Registry, name: &str) -> Result<String> {
    let mouth = Mouth::from_reg(reg, name)?;
    let _lk = dir_lock()?;
    if ours_running(&mouth) {
        match health(mouth.entry.port) {
            Some(200) => {
                return Ok(format!(
                    "menagerie [{}] ready at {} (model: {})",
                    mouth.app, mouth.url(), mouth.model_name
                ));
            }
            Some(503) => {
                if mouth.entry.backend == "gpu" {
                    gpu_arbitrate(reg, &mouth)?;
                }
                return Ok(format!(
                    "menagerie [{}] asleep at {} — next request wakes it",
                    mouth.app,
                    mouth.url()
                ));
            }
            _ => {}
        }
    }
    if mouth.entry.backend == "gpu" {
        gpu_arbitrate(reg, &mouth)?;
    }
    ram_fit(reg, &mouth)?;
    start_locked(&mouth)
}

pub fn start(reg: &Registry, name: &str) -> Result<String> {
    let mouth = Mouth::from_reg(reg, name)?;
    let _lk = dir_lock()?;
    start_locked(&mouth)
}

fn start_locked(mouth: &Mouth) -> Result<String> {
    if ours_running(mouth) && health(mouth.entry.port) == Some(200) {
        return Ok(format!(
            "menagerie [{}] already running at {}",
            mouth.app,
            mouth.url()
        ));
    }
    if port_held_by_stranger(mouth) {
        bail!(
            "port {} is held by another process — not ours, refusing to touch it",
            mouth.entry.port
        );
    }
    if !mouth.model_path.is_file() {
        bail!("model file missing: {}", mouth.model_path.display());
    }
    let server = backend::find_server(&mouth.entry.backend)?;
    let spec = SpawnSpec {
        app: mouth.app.clone(),
        model: mouth.model_path.clone(),
        host: HOST.into(),
        port: mouth.entry.port,
        ctx: mouth.entry.ctx,
        threads: threads(),
        threads_batch: threads_batch(),
        ngl: if mouth.entry.backend == "gpu" { 99 } else { 0 },
        alias: if mouth.entry.alias.is_empty() {
            mouth.app.clone()
        } else {
            mouth.entry.alias.clone()
        },
        sleep_idle: env_u32("PIXIE_LLM_SLEEP_IDLE", 300) as i64,
        seed: mouth.entry.seed.unwrap_or(1),
        backend: mouth.entry.backend.clone(),
    };
    let argv = backend::llama_argv(&server, &spec);
    rotate_log(&paths::log_file(&mouth.app))?;
    fs::create_dir_all(paths::llm_dir())?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o644)
        .open(paths::log_file(&mouth.app))?;
    let log_err = log.try_clone()?;
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    let mut child = cmd.spawn().with_context(|| format!("spawn {}", argv[0]))?;
    let pid = child.id();
    thread::spawn(move || {
        let _ = child.wait();
    });
    thread::sleep(Duration::from_millis(50));
    let starttime = slot::read_starttime(&slot::default_proc(), pid).unwrap_or(0);
    slot::save_identity(
        &paths::pid_file(&mouth.app),
        &paths::stamp_file(&mouth.app),
        &Identity { pid, starttime },
    )?;
    for _ in 0..90 {
        if health(mouth.entry.port).is_some() {
            return Ok(format!(
                "ready: {} (pid {pid}) [{}]\nlogs:  {}",
                mouth.url(),
                mouth.app,
                paths::log_file(&mouth.app).display()
            ));
        }
        if !slot::pid_alive(&slot::default_proc(), pid) {
            bail!(
                "llama-server [{}] exited — see {}",
                mouth.app,
                paths::log_file(&mouth.app).display()
            );
        }
        thread::sleep(Duration::from_secs(1));
    }
    bail!("timeout waiting for {} /health", mouth.url())
}

pub fn stop(reg: &Registry, name: &str) -> Result<String> {
    let mouth = Mouth::from_reg(reg, name)?;
    let _lk = dir_lock()?;
    stop_unlocked(&mouth)
}

fn stop_unlocked(mouth: &Mouth) -> Result<String> {
    let Some(id) = slot::load_identity(&paths::pid_file(&mouth.app), &paths::stamp_file(&mouth.app))
    else {
        return Ok(format!("menagerie [{}] already stopped", mouth.app));
    };
    if !slot::is_ours(&slot::default_proc(), &id, mouth.entry.port) {
        let _ = fs::remove_file(paths::pid_file(&mouth.app));
        let _ = fs::remove_file(paths::stamp_file(&mouth.app));
        return Ok(format!(
            "menagerie [{}] pid {} is not ours — left it alone",
            mouth.app, id.pid
        ));
    }
    slot::kill_if_ours(&slot::default_proc(), &id, mouth.entry.port, libc::SIGTERM)?;
    for _ in 0..40 {
        if !slot::pid_alive(&slot::default_proc(), id.pid) {
            let _ = fs::remove_file(paths::pid_file(&mouth.app));
            let _ = fs::remove_file(paths::stamp_file(&mouth.app));
            return Ok(format!("menagerie [{}] stopped (term)", mouth.app));
        }
        thread::sleep(Duration::from_millis(250));
    }
    slot::kill_if_ours(&slot::default_proc(), &id, mouth.entry.port, libc::SIGKILL)?;
    let _ = fs::remove_file(paths::pid_file(&mouth.app));
    let _ = fs::remove_file(paths::stamp_file(&mouth.app));
    Ok(format!("menagerie [{}] stopped (kill)", mouth.app))
}

pub fn status_line(reg: &Registry, name: &str) -> String {
    match Mouth::from_reg(reg, name) {
        Err(e) => format!("{name}\t?\t?\terror\t{e}"),
        Ok(m) => {
            let st = if ours_running(&m) {
                match health(m.entry.port) {
                    Some(200) => "up",
                    Some(503) => "asleep",
                    _ => "stale",
                }
            } else if crate::netport::listening(m.entry.port) {
                "busy"
            } else {
                "down"
            };
            let pid = slot::load_identity(&paths::pid_file(&m.app), &paths::stamp_file(&m.app))
                .map(|i| i.pid.to_string())
                .unwrap_or_else(|| "-".into());
            format!(
                "{}\t{}\t{}\t{}\t{}",
                m.app, m.model_name, m.entry.port, st, pid
            )
        }
    }
}

fn ram_fit(reg: &Registry, mouth: &Mouth) -> Result<()> {
    let budget = reg.ram_budget_gb;
    if budget <= 0.0 {
        return Ok(());
    }
    let mut loaded = mouth.size;
    for (name, app) in &reg.apps {
        if name == &mouth.app {
            continue;
        }
        if let Ok(m) = Mouth::from_reg(reg, name) {
            if ours_running(&m) {
                loaded += m.size;
            }
        }
        let _ = app;
    }
    let gb = vram::ram_loaded_gb(&[loaded]);
    if gb <= budget + 0.05 {
        return Ok(());
    }
    bail!(
        "RAM budget ( {} ) exceeded (loaded {}) — stop another app or raise it: menagerie budget",
        vram::format_loaded(budget),
        vram::format_loaded(gb)
    )
}

fn gpu_arbitrate(reg: &Registry, mouth: &Mouth) -> Result<()> {
    let need = match mouth.vram_gb {
        Some(n) => n,
        None => {
            eprintln!("warning: no VRAM cost for {} — proceeding blind", mouth.model_name);
            return Ok(());
        }
    };
    let Some(mut free) = vram::vram_free_gb() else {
        eprintln!("warning: no VRAM sensor — proceeding blind");
        return Ok(());
    };
    if free >= need {
        return Ok(());
    }
    // park lower priority gpu holders
    let mut names: Vec<String> = reg.apps.keys().cloned().collect();
    names.sort_by_key(|n| vram::prio_rank(n));
    for holder in names {
        if holder == mouth.app {
            continue;
        }
        if vram::prio_rank(&mouth.app) >= vram::prio_rank(&holder) {
            continue;
        }
        let Ok(h) = Mouth::from_reg(reg, &holder) else {
            continue;
        };
        if h.entry.backend != "gpu" {
            continue;
        }
        if health(h.entry.port) != Some(200) {
            continue;
        }
        eprintln!(
            "menagerie: {} needs the card ({need:.1} GB, {free:.1} free) — parking {holder}…",
            mouth.app
        );
        let _ = stop_unlocked(&h);
        if let Some(f) = vram::vram_free_gb() {
            free = f;
            if free >= need {
                return Ok(());
            }
        }
    }
    bail!(
        "VRAM short ({free:.1} GB free, need {need:.1}) — stop something and retry"
    )
}

fn rotate_log(path: &Path) -> Result<()> {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > 2 * 1024 * 1024 {
            let old = path.with_extension("log.old");
            let _ = fs::rename(path, old);
        }
    }
    Ok(())
}

struct DirLock(std::fs::File);

fn dir_lock() -> Result<DirLock> {
    fs::create_dir_all(paths::llm_dir())?;
    let p = paths::llm_dir().join(".lock");
    let f = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&p)?;
    let rc = unsafe { libc::flock(std::os::unix::io::AsRawFd::as_raw_fd(&f), libc::LOCK_EX) };
    if rc != 0 {
        bail!("menagerie state dir locked");
    }
    Ok(DirLock(f))
}

impl Drop for DirLock {
    fn drop(&mut self) {
        unsafe { libc::flock(std::os::unix::io::AsRawFd::as_raw_fd(&self.0), libc::LOCK_UN) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stranger_port_message() {
        let m = Mouth {
            app: "pixie".into(),
            entry: crate::config::AppEntry {
                model: "m".into(),
                port: 1,
                ctx: 8,
                alias: "qwen".into(),
                backend: "cpu".into(),
                seed: None,
            },
            model_path: "/nope".into(),
            model_name: "m".into(),
            vram_gb: None,
            size: 1,
        };
        // port 1 is not a llama mouth; stranger check is false unless something listens
        let _ = port_held_by_stranger(&m);
    }
}
