//! AMD sysfs VRAM sensor. One 8 GB class card. Priority pixie > magpie.

use std::fs;
use std::path::Path;

pub const GPU_PRIORITY: &[&str] = &["pixie", "magpie"];

pub fn prio_rank(app: &str) -> usize {
    GPU_PRIORITY
        .iter()
        .position(|a| *a == app)
        .unwrap_or(99)
}

pub fn vram_free_gb() -> Option<f64> {
    vram_free_gb_from(Path::new("/sys/class/drm"))
}

pub fn vram_free_gb_from(drm: &Path) -> Option<f64> {
    let rd = fs::read_dir(drm).ok()?;
    for ent in rd.flatten() {
        let d = ent.path().join("device");
        let total_p = d.join("mem_info_vram_total");
        let used_p = d.join("mem_info_vram_used");
        if !total_p.is_file() || !used_p.is_file() {
            continue;
        }
        let total: f64 = fs::read_to_string(total_p).ok()?.trim().parse().ok()?;
        let used: f64 = fs::read_to_string(used_p).ok()?.trim().parse().ok()?;
        if total > 0.0 {
            return Some((total - used) / 1024.0 / 1024.0 / 1024.0);
        }
    }
    None
}

/// RAM estimate: sum of model file sizes for listed loaded apps, in GB.
/// Labeled estimate — not RSS.
pub fn ram_loaded_gb(sizes: &[i64]) -> f64 {
    sizes.iter().copied().sum::<i64>() as f64 / 1_073_741_824.0
}

pub fn format_loaded(gb: f64) -> String {
    format!("{gb:.2} GB")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixie_outranks_magpie() {
        assert!(prio_rank("pixie") < prio_rank("magpie"));
        assert!(prio_rank("magpie") < prio_rank("imp"));
    }

    #[test]
    fn loaded_unit_is_single_gb() {
        let s = format_loaded(7.01);
        assert_eq!(s.matches("GB").count(), 1, "{s}");
        assert!(!s.contains("GB GB"), "{s}");
    }

    #[test]
    fn sysfs_free_from_fixture() {
        let root = std::env::temp_dir().join(format!("drm-{}", std::process::id()));
        let card = root.join("card0/device");
        fs::create_dir_all(&card).unwrap();
        // 8 GiB total, 2 GiB used
        fs::write(card.join("mem_info_vram_total"), "8589934592").unwrap();
        fs::write(card.join("mem_info_vram_used"), "2147483648").unwrap();
        let free = vram_free_gb_from(&root).unwrap();
        assert!((free - 6.0).abs() < 0.01, "{free}");
        let _ = fs::remove_dir_all(&root);
    }
}
