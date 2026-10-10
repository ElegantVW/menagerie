package config

import (
	"os"
	"path/filepath"
	"sync"
	"testing"
)

func TestRoundtrip(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "menagerie.json")
	reg := &Registry{
		Version:     1,
		RAMBudgetGB: 60.7,
		BudgetSeen:  true,
		Models:      map[string]ModelEntry{"m": {Path: "/x/y.gguf", Size: 42}},
		Apps:        map[string]AppEntry{"magpie": {Model: "m", Port: 8091, Ctx: 8192, Alias: "magpie"}},
	}
	if err := Save(p, reg); err != nil {
		t.Fatal(err)
	}
	got, err := Load(p)
	if err != nil {
		t.Fatal(err)
	}
	if got.Apps["magpie"].Port != 8091 || got.RAMBudgetGB != 60.7 || got.Models["m"].Size != 42 {
		t.Fatalf("roundtrip lost data: %+v", got)
	}
}

func TestCorruptFailsLoud(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "menagerie.json")
	os.WriteFile(p, []byte(`{"apps": {"magpie": {BROKEN`), 0o644)
	if _, err := Load(p); err == nil {
		t.Fatal("corrupt registry must error, never silently default")
	}
	if _, err := Load(filepath.Join(dir, "missing.json")); err == nil {
		t.Fatal("missing registry must error")
	}
}

func TestConcurrentWritersNeverTear(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "menagerie.json")
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			r := &Registry{Version: 1, RAMBudgetGB: float64(i), Apps: map[string]AppEntry{}, Models: map[string]ModelEntry{}}
			Save(p, r) // errors (lock contention) are fine; tears are not
		}(i)
	}
	wg.Wait()
	got, err := Load(p)
	if err != nil {
		t.Fatalf("torn write survived locking: %v", err)
	}
	if got.Version != 1 {
		t.Fatalf("garbage won: %+v", got)
	}
}
