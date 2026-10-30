# Changelog

## Unreleased

- `ensure` restarts the guest when the live cmdline `-m` path is not the registry model. `set` then `ensure` while another GGUF is already healthy used to report ready and keep serving the old weights. VRAM short is a real load refusal (`gpu_arbitrate` bails; llama-server is not started).
- Vision mouths pass `--mmproj` (registry field or a `*mmproj*.gguf` sibling). `ensure --slot vision` binds the VL GGUF. Eye ctx is 4096. Missing projector fails loud.
- Models have `kind`: llm | vision | image | tts. `ensure` refuses image/tts (sd-cli stays the Imp hand).
- Apps have `slots` (pixie chat/code/vision, imp llm/image, kur llm) and optional `ngl` for RAM/VRAM split.
- `menagerie set <app> <slot> <model>`, `menagerie slots <app>`, `ensure --slot`.
- Load infers kind from the filename and fills empty slots from the house pool.

- Rust steward (clap CLI). llama-server is a guest behind `backend::llama_argv`.
- Registry: atomic save + flock; corrupt/missing fails loud (audit #9).
- Slot identity: pid + starttime + cmdline; refuse to signal a recycled PID (audit #1, #2).
- Spawn argv drops `--chat-template` jinja. House parser owns tool JSON.
- GPU missing Vulkan wrapper is an error. RAM loaded string is a single `GB`.
- Go scaffold frozen; do not dual-maintain.
