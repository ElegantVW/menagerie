//! Registry at ~/.config/pixie/menagerie.json.
//! Atomic save (temp + fsync + rename) under flock. Corrupt/missing = error.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Registry {
    #[serde(default = "one")]
    pub version: i32,
    #[serde(default)]
    pub ram_budget_gb: f64,
    #[serde(default)]
    pub budget_seen: bool,
    #[serde(default)]
    pub models: BTreeMap<String, ModelEntry>,
    #[serde(default)]
    pub apps: BTreeMap<String, AppEntry>,
}

fn one() -> i32 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    #[default]
    Llm,
    Vision,
    Image,
    Tts,
}

impl ModelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelKind::Llm => "llm",
            ModelKind::Vision => "vision",
            ModelKind::Image => "image",
            ModelKind::Tts => "tts",
        }
    }

    pub fn is_llama(self) -> bool {
        matches!(self, ModelKind::Llm | ModelKind::Vision)
    }
}

pub fn infer_kind(name: &str, path: &str) -> ModelKind {
    let s = format!("{name} {path}").to_lowercase();
    if s.contains("tts") || s.contains("kokoro") || s.contains("whisper") {
        ModelKind::Tts
    } else if s.contains("pony")
        || s.contains("sdxl")
        || s.contains("sd_turbo")
        || s.contains("sd-turbo")
        || s.contains("qwen-image")
        || s.contains("qwen_image")
        || s.contains("flux")
        || s.contains("sd_xl")
    {
        ModelKind::Image
    } else if s.contains("-vl-") || s.contains("_vl_") || s.contains("vl-7b") || s.contains("vl-4b")
    {
        ModelKind::Vision
    } else {
        ModelKind::Llm
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelEntry {
    pub path: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_gb: Option<f64>,
    #[serde(default)]
    pub kind: ModelKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mmproj: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppEntry {
    pub model: String,
    pub port: u16,
    #[serde(default = "default_ctx")]
    pub ctx: u32,
    #[serde(default)]
    pub alias: String,
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default)]
    pub slots: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ngl: Option<u32>,
}

fn default_ctx() -> u32 {
    8192
}

fn default_backend() -> String {
    "cpu".into()
}

fn pick_model(reg: &Registry, needles: &[&str]) -> Option<String> {
    let lower: Vec<(String, String)> = reg
        .models
        .keys()
        .map(|k| (k.clone(), k.to_lowercase()))
        .collect();
    for n in needles {
        let n = n.to_lowercase();
        if let Some((k, _)) = lower.iter().find(|(_, l)| l.contains(&n)) {
            return Some(k.clone());
        }
    }
    None
}

pub fn migrate_slots(reg: &mut Registry) {
    let coder = pick_model(
        reg,
        &["qwen2.5-coder-3b-instruct-abliterated", "qwen2.5-coder-3b"],
    );
    let heretic = pick_model(reg, &["qwen3-0.6b-heretic", "0.6b-heretic"]);
    let vl = pick_model(reg, &["qwen2.5-vl-7b", "vl-7b"]);
    let turbo = pick_model(reg, &["sd_turbo-f16", "sd_turbo"]);
    if let Some(pixie) = reg.apps.get_mut("pixie") {
        if pixie.slots.is_empty() {
            pixie.slots.insert("chat".into(), pixie.model.clone());
            if let Some(c) = coder.clone() {
                pixie.slots.insert("code".into(), c);
            }
            if let Some(v) = vl {
                pixie.slots.insert("vision".into(), v);
            }
        }
    }
    if let Some(kur) = reg.apps.get_mut("kur") {
        if kur.slots.is_empty() {
            let m = heretic.clone().unwrap_or_else(|| kur.model.clone());
            kur.slots.insert("llm".into(), m.clone());
            if heretic.is_some() {
                kur.model = m;
                kur.backend = "cpu".into();
            }
        }
    }
    if let Some(imp) = reg.apps.get_mut("imp") {
        if imp.slots.is_empty() {
            let llm = coder.unwrap_or_else(|| imp.model.clone());
            imp.slots.insert("llm".into(), llm.clone());
            if let Some(img) = turbo {
                imp.slots.insert("image".into(), img);
            }
            imp.model = llm;
            imp.backend = "cpu".into();
        }
    }
}

impl Registry {
    pub fn default_path() -> PathBuf {
        if let Ok(p) = std::env::var("PIXIE_LLM_CFG") {
            return PathBuf::from(p).join("menagerie.json");
        }
        crate::paths::home().join(".config/pixie/menagerie.json")
    }

    pub fn resolve_app(&self, name: &str) -> Result<(&str, &AppEntry)> {
        let key = match name {
            "qwen" | "default" => "pixie",
            other => other,
        };
        self.apps
            .get_key_value(key)
            .map(|(k, v)| (k.as_str(), v))
            .with_context(|| format!("unknown app '{name}'"))
    }
}

pub fn load(path: &Path) -> Result<Registry> {
    with_lock(path, false, |f| {
        let mut buf = String::new();
        f.read_to_string(&mut buf)?;
        if buf.trim().is_empty() {
            bail!("registry empty: {}", path.display());
        }
        let mut reg: Registry = serde_json::from_str(&buf)
            .with_context(|| format!("registry corrupt: {}", path.display()))?;
        for app in reg.apps.values_mut() {
            if app.backend != "gpu" && app.backend != "cpu" {
                app.backend = "cpu".into();
            }
        }
        for (name, m) in reg.models.iter_mut() {
            if m.kind == ModelKind::Llm {
                m.kind = infer_kind(name, &m.path);
            }
        }
        migrate_slots(&mut reg);
        Ok(reg)
    })
}

pub fn save(path: &Path, reg: &Registry) -> Result<()> {
    with_lock(path, true, |_f| {
        let dir = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(".menagerie-{}.tmp", std::process::id()));
        let mut out = File::create(&tmp)?;
        serde_json::to_writer_pretty(&mut out, reg)?;
        out.write_all(b"\n")?;
        out.sync_all()?;
        drop(out);
        fs::rename(&tmp, path)?;
        Ok(())
    })
}

fn with_lock<T>(
    path: &Path,
    exclusive: bool,
    fn_: impl FnOnce(&mut File) -> Result<T>,
) -> Result<T> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    let how = if exclusive {
        libc::LOCK_EX | libc::LOCK_NB
    } else {
        libc::LOCK_SH | libc::LOCK_NB
    };
    let rc = unsafe { libc::flock(f.as_raw_fd(), how) };
    if rc != 0 {
        bail!("registry locked by another menagerie, refusing to race");
    }
    f.seek(SeekFrom::Start(0))?;
    let out = fn_(&mut f);
    unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_UN) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn sample() -> Registry {
        let mut models = BTreeMap::new();
        models.insert(
            "m".into(),
            ModelEntry {
                path: "/x/y.gguf".into(),
                size: 42,
                vram_gb: Some(1.5),
                kind: ModelKind::Llm,
                mmproj: None,
            },
        );
        let mut apps = BTreeMap::new();
        apps.insert(
            "magpie".into(),
            AppEntry {
                model: "m".into(),
                port: 8091,
                ctx: 8192,
                alias: "magpie".into(),
                backend: "cpu".into(),
                seed: None,
                slots: BTreeMap::new(),
                ngl: None,
            },
        );
        Registry {
            version: 1,
            ram_budget_gb: 60.7,
            budget_seen: true,
            models,
            apps,
        }
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile();
        let p = dir.join("menagerie.json");
        let reg = sample();
        save(&p, &reg).unwrap();
        let got = load(&p).unwrap();
        assert_eq!(got.apps["magpie"].port, 8091);
        assert_eq!(got.ram_budget_gb, 60.7);
        assert_eq!(got.models["m"].size, 42);
        assert_eq!(got.models["m"].vram_gb, Some(1.5));
    }

    #[test]
    fn corrupt_fails_loud() {
        let dir = tempfile();
        let p = dir.join("menagerie.json");
        fs::write(&p, r#"{"apps": {"magpie": {BROKEN"#).unwrap();
        assert!(load(&p).is_err(), "corrupt registry must error");
        assert!(
            load(&dir.join("missing.json")).is_err(),
            "missing must error"
        );
    }

    #[test]
    fn empty_fails_loud() {
        let dir = tempfile();
        let p = dir.join("menagerie.json");
        fs::write(&p, "").unwrap();
        assert!(load(&p).is_err());
    }

    #[test]
    fn qwen_alias_is_pixie() {
        let mut r = sample();
        r.apps.insert(
            "pixie".into(),
            AppEntry {
                model: "m".into(),
                port: 8080,
                ctx: 8192,
                alias: "qwen".into(),
                backend: "gpu".into(),
                seed: Some(1),
                slots: BTreeMap::new(),
                ngl: None,
            },
        );
        assert_eq!(r.resolve_app("qwen").unwrap().0, "pixie");
        assert_eq!(r.resolve_app("default").unwrap().1.port, 8080);
    }

    #[test]
    fn infer_image_and_vision() {
        assert_eq!(
            infer_kind("pony-v6-Q8_0", "/m/pony-v6-Q8_0.gguf"),
            ModelKind::Image
        );
        assert_eq!(
            infer_kind("sd_turbo-f16", "/m/sd_turbo-f16.gguf"),
            ModelKind::Image
        );
        assert_eq!(
            infer_kind(
                "Qwen2.5-VL-7B-Abliterated-Caption-it.i1-IQ4_XS",
                "/m/Qwen2.5-VL-7B.gguf"
            ),
            ModelKind::Vision
        );
        assert_eq!(
            infer_kind("qwen3-tts-12hz-0.6b", "/m/qwen3-tts.gguf"),
            ModelKind::Tts
        );
        assert_eq!(
            infer_kind("Huihui-Qwen3-8B", "/m/Huihui-Qwen3-8B.gguf"),
            ModelKind::Llm
        );
    }

    #[test]
    fn pixie_slots_round_trip() {
        let mut r = sample();
        r.models.insert(
            "coder".into(),
            ModelEntry {
                path: "/x/coder.gguf".into(),
                size: 1,
                vram_gb: None,
                kind: ModelKind::Llm,
                mmproj: None,
            },
        );
        r.apps.insert(
            "pixie".into(),
            AppEntry {
                model: "m".into(),
                port: 8080,
                ctx: 8192,
                alias: "pixie".into(),
                backend: "gpu".into(),
                seed: None,
                slots: BTreeMap::new(),
                ngl: None,
            },
        );
        migrate_slots(&mut r);
        assert_eq!(
            r.apps["pixie"].slots.get("chat").map(String::as_str),
            Some("m")
        );
        let dir = tempfile();
        let p = dir.join("menagerie.json");
        save(&p, &r).unwrap();
        let got = load(&p).unwrap();
        assert_eq!(
            got.apps["pixie"].slots.get("chat").map(String::as_str),
            Some("m")
        );
    }

    #[test]
    fn concurrent_writers_never_tear() {
        let dir = tempfile();
        let p = dir.join("menagerie.json");
        save(&p, &sample()).unwrap();
        let mut hs = Vec::new();
        for i in 0..8 {
            let p = p.clone();
            hs.push(thread::spawn(move || {
                let mut r = sample();
                r.ram_budget_gb = i as f64;
                let _ = save(&p, &r);
            }));
        }
        for h in hs {
            h.join().unwrap();
        }
        let got = load(&p).unwrap();
        assert_eq!(got.version, 1);
        assert!(got.apps.contains_key("magpie"));
    }

    fn tempfile() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "menagerie-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
}
