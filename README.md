# Menagerie — the house steward

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
```

## Laws

- Own tree. Not folded into Pixie.
- Registry `~/.config/pixie/menagerie.json` — same schema.
- Identity = pid + starttime + cmdline. Never kill a recycled PID.
- GPU without `~/.local/lib/llama-vulkan/llama-server-vk` is an error.
- `cargo test` does not talk to a live llama-server.

Go scaffold (`go.mod`, `main.go`, `internal/`) is frozen leftover.
