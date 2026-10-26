//! menagerie — house steward for per-app loopback model servers.

mod backend;
mod config;
mod netport;
mod paths;
mod serve;
mod slot;
mod vram;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use config::Registry;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "menagerie", version, about = "faeOS AI steward")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Summon an app's model if not healthy
    Ensure { app: String },
    Start { app: String },
    Stop { app: String },
    Restart { app: String },
    Status { app: String },
    Chat { app: String },
    Logs { app: String },
    Path { app: String },
    Set { app: String, model: String },
    Models {
        #[command(subcommand)]
        sub: Option<ModelsCmd>,
    },
    Budget { n: Option<f64> },
    Backend {
        app: String,
        which: Option<String>,
    },
}

#[derive(Subcommand)]
enum ModelsCmd {
    Add { path: PathBuf },
    Rm { name: String },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("menagerie: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        None => return bare_tui(),
        Some(Cmd::Ensure { app }) => {
            let reg = load()?;
            println!("{}", serve::ensure(&reg, &app)?);
        }
        Some(Cmd::Start { app }) => {
            let reg = load()?;
            println!("{}", serve::start(&reg, &app)?);
        }
        Some(Cmd::Stop { app }) => {
            let reg = load()?;
            println!("{}", serve::stop(&reg, &app)?);
        }
        Some(Cmd::Restart { app }) => {
            let reg = load()?;
            let _ = serve::stop(&reg, &app);
            println!("{}", serve::ensure(&reg, &app)?);
        }
        Some(Cmd::Status { app }) => {
            let reg = load()?;
            if app == "all" {
                for name in reg.apps.keys() {
                    println!("{}", serve::status_line(&reg, name));
                }
            } else {
                println!("{}", serve::status_line(&reg, &app));
            }
        }
        Some(Cmd::Chat { app }) => {
            let reg = load()?;
            let m = serve::Mouth::from_reg(&reg, &app)?;
            println!("{}", m.url());
        }
        Some(Cmd::Logs { app }) => {
            let p = paths::log_file(&resolve_name(&app)?);
            let s = fs::read_to_string(&p).unwrap_or_default();
            let lines: Vec<&str> = s.lines().collect();
            let start = lines.len().saturating_sub(80);
            for l in &lines[start..] {
                println!("{l}");
            }
        }
        Some(Cmd::Path { app }) => {
            let reg = load()?;
            let m = serve::Mouth::from_reg(&reg, &app)?;
            println!("{}", m.model_path.display());
        }
        Some(Cmd::Set { app, model }) => {
            let mut reg = load()?;
            let key = {
                let (k, _) = reg.resolve_app(&app)?;
                k.to_string()
            };
            if !reg.models.contains_key(&model) {
                bail!("unknown model '{model}' (see `menagerie models`)");
            }
            if let Some(e) = reg.apps.get_mut(&key) {
                e.model = model.clone();
            }
            config::save(&Registry::default_path(), &reg)?;
            println!("menagerie [{key}] model → {model}");
        }
        Some(Cmd::Models { sub: None }) => {
            let reg = load()?;
            println!("model\tpath\tsize\tvram_gb");
            for (name, m) in &reg.models {
                let v = m
                    .vram_gb
                    .map(|g| format!("{g:.1}"))
                    .unwrap_or_else(|| "-".into());
                println!("{name}\t{}\t{}\t{v}", m.path, m.size);
            }
        }
        Some(Cmd::Models {
            sub: Some(ModelsCmd::Add { path }),
        }) => add_model(path)?,
        Some(Cmd::Models {
            sub: Some(ModelsCmd::Rm { name }),
        }) => rm_model(&name)?,
        Some(Cmd::Budget { n: None }) => {
            let reg = load()?;
            println!("{} GB", reg.ram_budget_gb);
        }
        Some(Cmd::Budget { n: Some(n) }) => {
            let mut reg = load()?;
            reg.ram_budget_gb = n;
            config::save(&Registry::default_path(), &reg)?;
            println!("{n} GB");
        }
        Some(Cmd::Backend { app, which: None }) => {
            let reg = load()?;
            let (name, e) = reg.resolve_app(&app)?;
            println!("{name}: {}", e.backend);
        }
        Some(Cmd::Backend {
            app,
            which: Some(w),
        }) => {
            if w != "cpu" && w != "gpu" {
                bail!("backend must be cpu|gpu");
            }
            let mut reg = load()?;
            let key = {
                let (k, _) = reg.resolve_app(&app)?;
                k.to_string()
            };
            if let Some(e) = reg.apps.get_mut(&key) {
                e.backend = w.clone();
            }
            config::save(&Registry::default_path(), &reg)?;
            println!("{key}: {w}");
        }
    }
    Ok(())
}

fn load() -> Result<Registry> {
    config::load(&Registry::default_path())
}

fn resolve_name(app: &str) -> Result<String> {
    let reg = load()?;
    Ok(reg.resolve_app(app)?.0.to_string())
}

fn add_model(path: PathBuf) -> Result<()> {
    if !path.is_file() {
        bail!("not a file: {}", path.display());
    }
    let size = fs::metadata(&path)?.len() as i64;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model")
        .to_string();
    let mut reg = load()?;
    reg.models.insert(
        name.clone(),
        config::ModelEntry {
            path: path.display().to_string(),
            size,
            vram_gb: Some((size as f64 / 1_073_741_824.0) * 1.6),
        },
    );
    config::save(&Registry::default_path(), &reg)?;
    println!("registered {name}");
    println!("bind it to an app:  menagerie set <app> {name}");
    Ok(())
}

fn rm_model(name: &str) -> Result<()> {
    let mut reg = load()?;
    let users: Vec<_> = reg
        .apps
        .iter()
        .filter(|(_, a)| a.model == name)
        .map(|(k, _)| k.clone())
        .collect();
    if !users.is_empty() {
        bail!(
            "'{name}' is in use by {} — switch them first (`menagerie set <app> <model>`)",
            users.join(", ")
        );
    }
    if reg.models.remove(name).is_none() {
        bail!("unknown model '{name}'");
    }
    config::save(&Registry::default_path(), &reg)?;
    println!("unregistered {name}");
    Ok(())
}

fn bare_tui() -> Result<()> {
    let home = paths::home();
    for p in [
        home.join("bin/menagerie-tui.py"),
        home.join(".local/lib/faeos/menagerie-tui.py"),
    ] {
        if p.is_file() {
            let st = Command::new("python3").arg(&p).status()?;
            std::process::exit(st.code().unwrap_or(1));
        }
    }
    println!(
        "menagerie — the AI control center (per-app local model servers)

  menagerie ensure <app>     summon an app's model if not healthy
  menagerie start|stop|restart|status|chat|logs|path <app>
  menagerie set <app> <model>  switch which model an app uses
  menagerie models [add <path>] [rm <name>]
  menagerie budget [N]         RAM budget for loaded models (GB)
  menagerie backend <app> [cpu|gpu]

Apps: pixie→8080  kur→8081  imp→8082  magpie→8091
llama-server is a guest. Tool JSON is house-parsed (no llama jinja)."
    );
    Ok(())
}
