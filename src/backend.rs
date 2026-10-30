//! Inference backend. This cut: llama-server guest. HouseRunner is a stub.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SpawnSpec {
    #[allow(dead_code)]
    pub app: String,
    pub model: PathBuf,
    pub host: String,
    pub port: u16,
    pub ctx: u32,
    pub threads: u32,
    pub threads_batch: u32,
    pub ngl: u32,
    pub alias: String,
    pub sleep_idle: i64,
    pub seed: i64,
    pub backend: String,
    pub mmproj: Option<PathBuf>,
}

impl SpawnSpec {
    pub fn ngl_for_backend(&self) -> u32 {
        if self.backend != "gpu" {
            return 0;
        }
        if let Ok(s) = std::env::var("PIXIE_LLM_NGL") {
            if let Ok(n) = s.parse::<u32>() {
                return n;
            }
        }
        if self.ngl > 0 {
            self.ngl
        } else {
            99
        }
    }
}

pub fn find_server(backend: &str) -> Result<PathBuf> {
    if let Ok(p) = std::env::var("PIXIE_LLAMA_SERVER") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    if backend == "gpu" {
        let vk = crate::paths::home().join(".local/lib/llama-vulkan/llama-server-vk");
        if vk.is_file() {
            return Ok(vk);
        }
        bail!("backend=gpu but no Vulkan server at ~/.local/lib/llama-vulkan/llama-server-vk");
    }
    let home = crate::paths::home();
    for p in [
        home.join("bin/llama-server"),
        home.join(".local/lib/pixie/llama-server"),
        PathBuf::from("/usr/bin/llama-server"),
        PathBuf::from("/usr/local/bin/llama-server"),
    ] {
        if p.is_file() {
            return Ok(p);
        }
    }
    if let Ok(p) = which("llama-server") {
        return Ok(p);
    }
    bail!("llama-server not found")
}

fn which(name: &str) -> Result<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':') {
        let p = Path::new(dir).join(name);
        if p.is_file() {
            return Ok(p);
        }
    }
    bail!("{name} not on PATH")
}

/// Guest argv. No --chat-template: house parser owns tool calls.
pub fn llama_argv(server: &Path, spec: &SpawnSpec) -> Vec<String> {
    let ngl = spec.ngl_for_backend();
    let idle = if spec.sleep_idle <= 0 {
        -1
    } else {
        spec.sleep_idle
    };
    let mut a = vec![
        server.display().to_string(),
        "-m".into(),
        spec.model.display().to_string(),
        "--host".into(),
        spec.host.clone(),
        "--port".into(),
        spec.port.to_string(),
        "-c".into(),
        spec.ctx.to_string(),
        "-t".into(),
        spec.threads.to_string(),
        "-tb".into(),
        spec.threads_batch.to_string(),
        "-ngl".into(),
        ngl.to_string(),
        "--alias".into(),
        spec.alias.clone(),
        "--sleep-idle-seconds".into(),
        idle.to_string(),
        "--seed".into(),
        spec.seed.to_string(),
        "--timeout".into(),
        "300".into(),
    ];
    if let Some(mm) = &spec.mmproj {
        a.push("--mmproj".into());
        a.push(mm.display().to_string());
    }
    a
}

pub fn find_mmproj(model: &Path, explicit: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let dir = model.parent()?;
    let stem = model.file_stem()?.to_string_lossy().to_lowercase();
    let mut hits: Vec<PathBuf> = Vec::new();
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        let name = p.file_name()?.to_string_lossy().to_lowercase();
        if name.contains("mmproj") && name.ends_with(".gguf") && p.is_file() {
            hits.push(p);
        }
    }
    if hits.is_empty() {
        return None;
    }
    hits.sort_by_key(|p| {
        let n = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        let related = if n.contains(&stem.chars().take(12).collect::<String>()) {
            0
        } else {
            1
        };
        let f16 = if n.contains("f16") { 0 } else { 1 };
        (related, f16, n)
    });
    hits.into_iter().next()
}

pub fn house_runner_stub() -> Result<PathBuf> {
    bail!("HouseRunner: not this cut — llama-server stays the guest")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SpawnSpec {
        SpawnSpec {
            app: "pixie".into(),
            model: PathBuf::from("/m.gguf"),
            host: "127.0.0.1".into(),
            port: 8080,
            ctx: 8192,
            threads: 8,
            threads_batch: 16,
            ngl: 0,
            alias: "qwen".into(),
            sleep_idle: 300,
            seed: 7,
            backend: "cpu".into(),
            mmproj: None,
        }
    }

    #[test]
    fn argv_has_no_chat_template() {
        let a = llama_argv(Path::new("/bin/llama-server"), &spec());
        let joined = a.join(" ");
        assert!(!joined.contains("chat-template"), "{joined}");
        assert!(!joined.contains("jinja"), "{joined}");
        assert!(joined.contains("--seed 7"), "{joined}");
        assert!(joined.contains("--port 8080"), "{joined}");
    }

    #[test]
    fn gpu_default_ngl_is_99() {
        let mut s = spec();
        s.backend = "gpu".into();
        assert_eq!(s.ngl_for_backend(), 99);
        let a = llama_argv(Path::new("/vk"), &s);
        let i = a.iter().position(|x| x == "-ngl").unwrap();
        assert_eq!(a[i + 1], "99");
    }

    #[test]
    fn gpu_honors_split_ngl() {
        let mut s = spec();
        s.backend = "gpu".into();
        s.ngl = 24;
        assert_eq!(s.ngl_for_backend(), 24);
        let a = llama_argv(Path::new("/vk"), &s);
        let i = a.iter().position(|x| x == "-ngl").unwrap();
        assert_eq!(a[i + 1], "24");
    }

    #[test]
    fn gpu_missing_vulkan_errors() {
        std::env::remove_var("PIXIE_LLAMA_SERVER");
        let vk = crate::paths::home().join(".local/lib/llama-vulkan/llama-server-vk");
        match find_server("gpu") {
            Ok(p) if vk.is_file() => assert_eq!(p, vk),
            Err(e) => {
                let s = e.to_string();
                assert!(s.contains("Vulkan") || s.contains("gpu"), "{s}");
            }
            Ok(p) => panic!("unexpected gpu server {p:?}"),
        }
    }

    #[test]
    fn house_runner_not_this_cut() {
        assert!(house_runner_stub().is_err());
    }

    #[test]
    fn argv_omits_mmproj_when_unset() {
        let a = llama_argv(Path::new("/bin/llama-server"), &spec());
        assert!(!a.contains(&"--mmproj".into()), "{a:?}");
    }

    #[test]
    fn argv_has_mmproj_when_set() {
        let mut s = spec();
        s.mmproj = Some(PathBuf::from("/m/eye.mmproj-f16.gguf"));
        let a = llama_argv(Path::new("/bin/llama-server"), &s);
        let i = a.iter().position(|x| x == "--mmproj").expect("mmproj");
        assert_eq!(a[i + 1], "/m/eye.mmproj-f16.gguf");
    }

    #[test]
    fn find_mmproj_picks_f16_sibling() {
        let dir = std::env::temp_dir().join(format!("men-mmproj-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let model = dir.join("Qwen2.5-VL-7B.gguf");
        let mm = dir.join("Qwen2.5-VL-7B.mmproj-f16.gguf");
        std::fs::write(&model, b"x").unwrap();
        std::fs::write(&mm, b"y").unwrap();
        let hit = find_mmproj(&model, None).unwrap();
        assert_eq!(hit, mm);
        let explicit = dir.join("explicit.mmproj.gguf");
        std::fs::write(&explicit, b"z").unwrap();
        assert_eq!(
            find_mmproj(&model, Some(explicit.to_str().unwrap())).unwrap(),
            explicit
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
