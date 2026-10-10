# Menagerie — the house steward

One steward for the house's model servers: each app (pixie, ask, magpie,
imp, kur) owns a loopback instance on its own port, supervised from one
place. Go, single binary, no daemon.

**Status: scaffold.** The old steward (`~/bin/menagerie`, bash+Python)
stays on duty until acceptance passes. This binary changes nothing yet.

**Why Go:** the children speak CLI + HTTP + files, so the tongue only
needs rock-solid process control, trivial HTTP/JSON, and one static
binary. `os/exec` is the best seat in the house for spawning servers;
`encoding/json` + `net/http` are stdlib. Same blood argument as ever —
but here the maintainer-reads-at-2am argument won over Rust: plain loops,
fast builds, boring ops.

**Why not the others:** C (hand-rolled HTTP/JSON where reliability
matters most — no), Zig (young, churns), Ada (nobody here speaks it),
Nim/Crystal/D/Odin (small pools for a supervisor), staying bash+Python
(the audit: PID races, torn writes, no supervision, three tongues).

## Order (agreed)

1. Audit (`docs/AUDIT.md` — static done, live slots deferred while magpie runs).
2. Provisioner: pinned engine versions, verified downloads, atomic
   versioned installs — this kills the 647MB llama.cpp source tree.
3. Supervisor core: flock-locked state (pid+starttime identity), per-app
   `--seed`, graceful drain, log rotation.
4. CLI compat frozen (byte-identical shapes) + acceptance (full command
   surface + a magpie single-page process run).
5. TUI last (it imports the old registry — real rewrite, bubbletea).

## Build

```bash
go build -o menagerie .     # single binary, no install yet
go test ./...               # config locks + atomicity included
```

`build.sh install` arrives with the supervisor core — and even then it
installs BESIDE the old steward until acceptance, never over it.
