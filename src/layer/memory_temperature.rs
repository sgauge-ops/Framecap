use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

const HWMON_ROOT: &str = "/sys/class/hwmon";

#[derive(Debug, PartialEq)]
pub struct Hotspot {
    pub temperature_c: i32,
    pub saturated: bool,
}

pub fn read_hotspot() -> Result<Hotspot> {
    read_hotspot_from(Path::new(HWMON_ROOT))
}

fn read_hotspot_from(root: &Path) -> Result<Hotspot> {
    let mut matches = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("cannot read {}", root.display()))? {
        let directory = entry.context("cannot enumerate hwmon devices")?.path();
        let name_path = directory.join("name");
        let name = match fs::read_to_string(&name_path) {
            Ok(name) => name,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", name_path.display()))
            }
        };
        if name.trim() != "bc250_memory" {
            continue;
        }
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("cannot enumerate {}", directory.display()))?
        {
            let entry = entry.context("cannot enumerate bc250_memory sensors")?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(index) = name
                .strip_prefix("temp")
                .and_then(|name| name.strip_suffix("_label"))
                .filter(|index| !index.is_empty() && index.bytes().all(|c| c.is_ascii_digit()))
            else {
                continue;
            };
            let label = fs::read_to_string(entry.path())
                .with_context(|| format!("cannot read {}", entry.path().display()))?;
            if label.trim() == "VRAM Hotspot" {
                matches.push(directory.join(format!("temp{index}_input")));
            }
        }
    }
    let path = match matches.as_slice() {
        [path] => path,
        _ => bail!(
            "expected one bc250_memory VRAM Hotspot sensor, found {}",
            matches.len()
        ),
    };
    let value =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse_hotspot(&value).with_context(|| format!("invalid reading from {}", path.display()))
}

fn parse_hotspot(value: &str) -> Result<Hotspot> {
    let millidegrees: i32 = value
        .trim()
        .parse()
        .context("expected integer millidegrees")?;
    if !(-40_000..=120_000).contains(&millidegrees) || (millidegrees + 40_000) % 2_000 != 0 {
        bail!("temperature {millidegrees} m°C is outside the JEDEC code range");
    }
    Ok(Hotspot {
        temperature_c: millidegrees / 1000,
        saturated: millidegrees == 120_000,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "framecap-memory-hwmon-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn write(root: &Path, path: &str, value: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    #[test]
    fn discovers_the_labeled_hotspot_on_the_bc250_driver() {
        let root = root();
        write(&root, "hwmon0/name", "amdgpu\n");
        write(&root, "hwmon0/temp1_label", "VRAM Hotspot\n");
        write(&root, "hwmon0/temp1_input", "100000\n");
        write(&root, "hwmon8/name", "bc250_memory\n");
        write(&root, "hwmon8/temp5_label", "VRAM Hotspot\n");
        write(&root, "hwmon8/temp5_input", "46000\n");
        assert_eq!(
            read_hotspot_from(&root).unwrap(),
            Hotspot {
                temperature_c: 46,
                saturated: false
            }
        );
        fs::rename(root.join("hwmon8"), root.join("hwmon3")).unwrap();
        assert_eq!(read_hotspot_from(&root).unwrap().temperature_c, 46);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_missing_or_ambiguous_sensors_and_read_errors() {
        let root = root();
        write(&root, "hwmon1/name", "bc250_memory\n");
        assert!(read_hotspot_from(&root)
            .unwrap_err()
            .to_string()
            .contains("found 0"));
        write(&root, "hwmon1/temp1_label", "VRAM Hotspot\n");
        assert!(read_hotspot_from(&root)
            .unwrap_err()
            .to_string()
            .contains("cannot read"));
        write(&root, "hwmon1/temp1_input", "bad");
        assert!(read_hotspot_from(&root)
            .unwrap_err()
            .to_string()
            .contains("invalid reading"));
        write(&root, "hwmon2/name", "bc250_memory\n");
        write(&root, "hwmon2/temp1_label", "VRAM Hotspot\n");
        assert!(read_hotspot_from(&root)
            .unwrap_err()
            .to_string()
            .contains("found 2"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validates_jedec_codes_and_saturation() {
        for (value, temp, saturated) in [
            ("-40000", -40, false),
            ("0", 0, false),
            ("46000", 46, false),
            ("120000", 120, true),
        ] {
            assert_eq!(
                parse_hotspot(value).unwrap(),
                Hotspot {
                    temperature_c: temp,
                    saturated
                }
            );
        }
        for value in [
            "", "NaN", "-42000", "121000", "470000", "468000", "45000", "46001",
        ] {
            assert!(parse_hotspot(value).is_err(), "{value}");
        }
    }
}
