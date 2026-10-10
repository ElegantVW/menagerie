// menagerie — the house steward: per-app local model servers, supervised.
// Go rewrite of the bash+Python steward. Migration law: children notice
// NOTHING — same ports, URLs, CLI shapes. See docs/AUDIT.md for why.
package main

import (
	"fmt"
	"os"
)

const version = "0.0.0-scaffold"

func main() {
	if len(os.Args) < 2 {
		help()
		return
	}
	switch os.Args[1] {
	case "--help", "-h", "help":
		help()
	case "--version":
		fmt.Println("menagerie", version)
	default:
		// Full command surface (ensure/start/stop/restart/status/chat/logs/
		// path/set/models/budget) lands with the supervisor core. Until
		// then the old steward stays on duty — this binary changes nothing.
		fmt.Fprintln(os.Stderr, "menagerie: not built yet (see docs/AUDIT.md) — ~/bin/menagerie stays on duty")
		os.Exit(1)
	}
}

func help() {
	fmt.Println(`menagerie — the AI control center (per-app local model servers)

  menagerie ensure <app>     summon an app's model if not healthy
  menagerie start|stop|restart|status|chat|logs|path <app>
  menagerie set <app> <model>  switch which model an app uses
  menagerie models [add <path>] [rm <name>]  model registry
  menagerie budget [N]         RAM budget for loaded models (GB)

Apps (each owns its own server instance):
  pixie→8080  ask→8090  magpie→8091  imp→8082  kur→8081`)
}
