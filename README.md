# Menagerie — the steward 🐾

Per-app loopback mouths (pixie `:8080`, kur `:8081`, imp `:8082`, magpie `:8091`),
RAM/VRAM budget, `ensure` / `stop` / `set` / `models`. **Rust.** llama-server is
a guest binary this cut. Tool JSON is house-parsed (Pixie); we do not send
OpenAI native `tools` to llama.cpp.

The old bash+Python steward (`~/bin/menagerie`) stays on PATH until
`./build.sh install --house` parks it as `~/.local/lib/faeos/menagerie-face`.

## Build

```bash
cd ~/menagerie
cargo test
./build.sh                  # release ELF
./build.sh install --house  # ELF + thin launcher; parks bash steward
```

```
menagerie ensure pixie
menagerie status all
menagerie stop pixie
menagerie set pixie code Qwen2.5-Coder-3B-Instruct-abliterated-Q4_K_M
menagerie slots pixie
```

Models have a `kind` (`llm` / `vision` / `image` / `tts`). `ensure` will not spawn llama-server for image or tts. Imp's image hand is sd-cli; the slot only stores the path. Vision mouths pass `--mmproj` (registry field or a `*mmproj*.gguf` sibling); missing projector fails loud. Eye ctx is 4096. One GPU tenant: hop parks the resident mouth first. `ensure pixie --slot vision` then `--slot chat` restores the 8B.

## Laws

- Own tree. Not folded into Pixie.
- Registry `~/.config/pixie/menagerie.json` — same schema.
- Identity = pid + starttime + cmdline. Never kill a recycled PID.
- GPU without `~/.local/lib/llama-vulkan/llama-server-vk` is an error.
- `cargo test` does not talk to a live llama-server.

Go scaffold (`go.mod`, `main.go`, `internal/`) is frozen leftover.
