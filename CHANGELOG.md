# Changelog

## Unreleased

- Rust steward (clap CLI). llama-server is a guest behind `backend::llama_argv`.
- Registry: atomic save + flock; corrupt/missing fails loud (audit #9).
- Slot identity: pid + starttime + cmdline; refuse to signal a recycled PID (audit #1, #2).
- Spawn argv drops `--chat-template` jinja. House parser owns tool JSON.
- GPU missing Vulkan wrapper is an error. RAM loaded string is a single `GB`.
- Go scaffold frozen; do not dual-maintain.
