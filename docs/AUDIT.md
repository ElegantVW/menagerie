# AUDIT — menagerie (the steward)

Evidence ledger. Every finding graded: **proven** (observed live),
**latent** (code path confirmed, trigger absent today), **deferred**
(needs a live experiment that would disturb running work — reason noted).

Steward today: `~/bin/menagerie` (443-line bash) + `menagerie-registry.py`
(526-line Python: config/budget/models) + `menagerie-run` (48-line bash
launcher) + `menagerie-tui.py` (269-line Python TUI, imports the registry
as a module — deep coupling, TUI rewrite is real work). Backend:
llama.cpp (647MB source tree + 187MB Vulkan runtime + 3.9MB headers).

## Findings

### 1. PID files trust recycled PIDs (latent → kill hazard)
`cmd_stop` does `kill $pid` on whatever number sits in
`llama-server.<app>.pid`, guarded only by `kill -0` (any live process
qualifies). 2026-10-10: ask/imp/kur PID files all stale-but-dead —
harmless today. On a busy box PIDs recycle; `menagerie stop ask` can
murder an innocent process. Fix in rewrite: state records PID **plus
process start time** (`/proc/<pid>/stat` field 22); mismatch = not ours,
never signal it. flock-locked state, never bare PID files.

### 2. Port presence counts as ownership (latent)
`is_running` falls back to `port_busy`: ANY process on the port reads as
"our slot running" (status lies; `ensure` then refuses to start a real
one: "already running"). Fix: identity = (pid, starttime, cmdline match
on `llama-server` + our port flag), never the port alone.

### 3. Concurrent ensures race (latent)
No lock anywhere: two `ensure` invocations both pass `is_running=false`,
both spawn, last PID file wins, orphaned server holds the port forever
("refusing to touch it" — the orphan becomes immortal). Fix: flock on
the state dir around check-and-spawn (single critical section).

### 4. No supervision between invocations (proven by design)
One-shot CLI: a mid-manual crash is discovered only at the next `ensure`;
every in-flight call fails one by one (lived through this: a full manual
of fallbacks). No daemon is smuggled in to fix it (house law); the rewrite
makes detection exact (proc validation) and recovery fast (healthy-path
`ensure` <50ms), with an opt-in `watch` later if wanted.

### 5. Unbounded logs (proven)
`llama-server.magpie.log` grew 515KB in 6h of one run. Months of runs =
tens of MB per app, never rotated. Fix: size-capped rotation in the
spawn path (~40 lines, house style), keep last N.

### 6. HF downloads unverified (proven by reading)
`add_model_hf` registers filename + size only. No hash, no signature.
Ceiling (honest): HF publishes no universal checksums, so the rewrite
records SHA256 on first download and verifies on reuse + shows on demand.
Full supply-chain is unachievable here; claimed nowhere.

### 7. Seed lottery (proven, incident-driven)
llama-server seeds randomly per boot; at temp 0.1 this once scored a
whole manual into the bin (magpie pinned temp 0.0 to survive it). Fix:
`--seed <per-app-fixed>` in spawn args (runner change + registry field).
Deterministic steward, deterministic children.

### 8. kill -9 after 1s (latent)
`cmd_stop` escalates to SIGKILL after one second — mid-generation work
wasted, no graceful drain. Fix: SIGTERM, wait for port release + process
exit (generous), SIGKILL only past the deadline, and say which happened.

### 9. Torn registry writes rebuild to defaults (latent, data-loss chain)
`save()` is a bare `write_text`; a crash mid-write leaves half-JSON.
`load()` catches the decode error → returns `{}` → `ensure_registry`
REBUILDS DEFAULTS — user bindings and budget silently replaced by
auto-suggestions. Fix: atomic write (temp + fsync + rename) under flock.

### 10. Budget math is file-size fiction (proven)
`est-loaded` counts full GGUF bytes as RAM-resident when "loaded" — but
NGL=99 slots hold weights in VRAM, and KV is a flat 0.25GB guess either
way. Works only because the budget (60.7GB) dwarfs reality. Bonus bug
spotted live: `loaded now: 7.01 GB GB` (double unit). Fix: measure, don't
estimate — parse server `/props` + `/v1/models`? (llama.cpp exposes
model size in props; resident = VRAM report unavailable → budget on
model size MINUS offload estimate is still a guess; honest fix = track
per-slot NGL + file size and label it estimate, plus `nvidia-smi`/VRAM
reading where available). At minimum: fix the double unit, document the
fiction.

### 11. `status all` is serially slow (proven by design)
5 apps × 1.5s curl timeouts = 7.5s worst case when boxes are down, plus
one python3 spawn per app resolution. Fix: parallel probes with short
budgets in one process (Go: goroutines, trivially).

### 12. TUI imports the registry (proven by reading)
`menagerie-tui.py` loads `menagerie-registry.py` as a module — the Go
rewrite cannot keep it alive. TUI is rebuilt last (bubbletea), CLI frozen
throughout so the old TUI keeps working against the OLD core until cutover.

## Deferred live experiments (magpie run owns :8091 — no disturbance)
- D1 double-ensure race on a scratch app (needs registry add + spawn).
- D2 stale-PID write + `stop` behavior (needs PID file control).
- D3 seed determinism across restarts at temp>0 (needs slot restart).
- D4 mid-generation SIGKILL + discovery latency (needs a loaded slot).
- D5 upstream Vulkan prebuilt availability + hashes (network check, safe
  anytime — do before provisioner build).

## Migration law (prime)
Children (pixie/magpie/imp/kur/ask) notice NOTHING: same ports, same URLs,
same CLI shapes and outputs. Rollback (old engine dir + old script) stays
until acceptance: ensure/start/stop/status/chat/path + a magpie single-page
process run, all green.
