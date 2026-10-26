use std::path::PathBuf;

pub fn home() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h);
    }
    PathBuf::from("/home/evenweaker")
}

pub fn llm_dir() -> PathBuf {
    if let Ok(p) = std::env::var("PIXIE_LLM_CACHE") {
        return PathBuf::from(p).join("llm");
    }
    home().join(".cache/pixie/llm")
}

#[allow(dead_code)]
pub fn model_dir() -> PathBuf {
    if let Ok(p) = std::env::var("PIXIE_LLM_MODEL_DIR") {
        return PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("PIXIE_LLM_DATA") {
        return PathBuf::from(p).join("models");
    }
    home().join(".local/share/pixie/models")
}

pub fn pid_file(app: &str) -> PathBuf {
    llm_dir().join(format!("llama-server.{app}.pid"))
}

pub fn stamp_file(app: &str) -> PathBuf {
    llm_dir().join(format!("llama-server.{app}.stamp"))
}

pub fn log_file(app: &str) -> PathBuf {
    llm_dir().join(format!("llama-server.{app}.log"))
}
