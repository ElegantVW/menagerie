# Menagerie — the steward 🐾

```
[] [] []
|  |  |
|  |  |
```

Local llama steward for faeOS. **Rust.** One GPU tenant, loopback mouths, RAM/VRAM budget. llama-server is a guest binary — Menagerie does not speak to the cloud.

For: waking Pixie / Kur / Imp / Magpie mouths on loopback. Not for: hosting models on the public internet, folding this into Pixie.

```
git clone git@github.com:ElegantVW/menagerie.git ~/menagerie
cd ~/menagerie && cargo test --offline && ./build.sh install --house
menagerie status all
menagerie ensure pixie
```

The old bash+Python steward stays on PATH until house-install parks it as `menagerie-face`. Go scaffold in this tree is frozen leftover — the live engine is the Rust ELF.

## Look

```
menagerie ensure pixie
menagerie status all
menagerie stop pixie
menagerie set pixie code <model>
menagerie slots pixie
```

Models have a `kind` (`llm` / `vision` / `image` / `tts`). `ensure` will not spawn llama-server for image or tts. Vision mouths pass `--mmproj`; missing projector fails loud. One GPU tenant: hop parks the resident mouth first.

## Laws

- Own tree. Not folded into Pixie.
- Registry `~/.config/pixie/menagerie.json` — same schema.
- Identity = pid + starttime + cmdline. Never kill a recycled PID.
- `cargo test` does not talk to a live llama-server.

## License

MIT — see [LICENSE](LICENSE).
