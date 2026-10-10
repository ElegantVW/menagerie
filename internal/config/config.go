// Package config owns the registry file (~/.config/pixie/menagerie.json).
// Schema matches the Python registry EXACTLY (migration law: same bytes).
// Two laws the old code broke: writes are atomic (temp + fsync + rename —
// a crash mid-write never leaves half-JSON), and corrupt files FAIL LOUD
// (never silently rebuilt into defaults — that chain ate user configs).
package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"syscall"
)

// Registry mirrors menagerie-registry.py's on-disk shape, field for field.
type Registry struct {
	Version     int                    `json:"version"`
	RAMBudgetGB float64                `json:"ram_budget_gb"`
	BudgetSeen  bool                   `json:"budget_seen"`
	Models      map[string]ModelEntry  `json:"models"`
	Apps        map[string]AppEntry    `json:"apps"`
}

type ModelEntry struct {
	Path string `json:"path"`
	Size int64  `json:"size"`
}

type AppEntry struct {
	Model string `json:"model"`
	Port  int    `json:"port"`
	Ctx   int    `json:"ctx"`
	Alias string `json:"alias"`
}

func DefaultPath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		home = "/home/evenweaker"
	}
	if p := os.Getenv("PIXIE_LLM_CFG"); p != "" {
		return filepath.Join(p, "menagerie.json")
	}
	return filepath.Join(home, ".config", "pixie", "menagerie.json")
}

// withLock opens path (creating parents) and holds an flock for fn's run.
// Shared for reads, exclusive for writes. Locking the registry file itself
// (not a sidecar) means stale lockfiles can never wedge the steward.
func withLock(path string, exclusive bool, fn func(f *os.File) error) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	how := syscall.LOCK_SH
	if exclusive {
		how = syscall.LOCK_EX
	}
	if err := syscall.Flock(int(f.Fd()), how|syscall.LOCK_NB); err != nil {
		return fmt.Errorf("registry locked by another menagerie, refusing to race: %w", err)
	}
	defer syscall.Flock(int(f.Fd()), syscall.LOCK_UN)
	return fn(f)
}

// Load reads the registry under a shared lock. Corrupt or missing files
// are ERRORS — the caller decides (migrate/seed/abort), never silent defaults.
func Load(path string) (*Registry, error) {
	var reg Registry
	err := withLock(path, false, func(f *os.File) error {
		st, err := f.Stat()
		if err != nil {
			return err
		}
		if st.Size() == 0 {
			return fmt.Errorf("registry empty: %s", path)
		}
		return json.NewDecoder(f).Decode(&reg)
	})
	if err != nil {
		return nil, err
	}
	if reg.Models == nil {
		reg.Models = map[string]ModelEntry{}
	}
	if reg.Apps == nil {
		reg.Apps = map[string]AppEntry{}
	}
	return &reg, nil
}

// Save writes atomically under an exclusive lock: temp file in the same
// directory, fsync, rename over the original. A crash lands either the old
// file or the new one — never half-JSON (finding #9, buried here).
func Save(path string, reg *Registry) error {
	return withLock(path, true, func(_ *os.File) error {
		dir := filepath.Dir(path)
		tmp, err := os.CreateTemp(dir, ".menagerie-*.tmp")
		if err != nil {
			return err
		}
		tmpName := tmp.Name()
		enc := json.NewEncoder(tmp)
		enc.SetIndent("", "  ")
		if err := enc.Encode(reg); err != nil {
			tmp.Close()
			os.Remove(tmpName)
			return err
		}
		if err := tmp.Sync(); err != nil {
			tmp.Close()
			os.Remove(tmpName)
			return err
		}
		if err := tmp.Close(); err != nil {
			os.Remove(tmpName)
			return err
		}
		return os.Rename(tmpName, path)
	})
}
