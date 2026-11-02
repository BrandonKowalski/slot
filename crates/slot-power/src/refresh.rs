use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

const DIVIDER: u32 = 0x0651_1044;
const PLL: u32 = 0x0300_1040;

pub(crate) struct PanelRefresh {
    dump: PathBuf,
    write: PathBuf,
    desired: Option<bool>,
}

impl PanelRefresh {
    pub(crate) fn probe(sysfs: &Path, target: &str) -> Option<Self> {
        let driver = match target {
            "rgsp" | "rg34xxsp" => b"rg34xxsp_v1\0".as_slice(),
            "rg34xx" => b"fog_fj035fhd05_v1\0".as_slice(),
            _ => return None,
        };
        let tree = sysfs.join("firmware/devicetree/base");
        let alias = fs::read(tree.join("aliases/lcd0")).ok()?;
        let aliases = alias.strip_suffix(&[0])?;
        let mut aliases = aliases.split(|&b| b == 0);
        let first = aliases.next()?;
        if first.is_empty() || aliases.any(|entry| entry != first) {
            return None;
        }
        let alias = std::str::from_utf8(first).ok()?;
        let relative = Path::new(alias.strip_prefix('/')?);
        if !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return None;
        }
        let panel = tree.join(relative);
        if fs::read(panel.join("lcd_driver_name")).ok()?.as_slice() != driver {
            return None;
        }
        for (name, expected) in [
            ("lcd_x", 720),
            ("lcd_y", 480),
            ("lcd_ht", 820),
            ("lcd_vt", 536),
            ("lcd_dclk_freq", 26),
        ] {
            let bytes = fs::read(panel.join(name)).ok()?;
            if u32::from_be_bytes(bytes.try_into().ok()?) != expected {
                return None;
            }
        }
        let dump = sysfs.join("class/sunxi_dump/dump");
        let write = sysfs.join("class/sunxi_dump/write");
        if !writable(&dump) || !writable(&write) {
            return None;
        }
        Some(PanelRefresh {
            dump,
            write,
            desired: None,
        })
    }

    pub(crate) fn set(&mut self, high: bool) -> bool {
        let result = apply(
            high,
            |address, value| fs::write(&self.write, format!("0x{address:08x} 0x{value:08x}\n")),
            |mode| self.verify(mode),
        );
        self.desired = Some(high && result.is_ok());
        match result {
            Ok(()) => true,
            Err(e) => {
                eprintln!("slot: refresh: {e}");
                false
            }
        }
    }

    pub(crate) fn resume(&mut self) {
        if let Some(high) = self.desired {
            self.set(high);
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.desired.unwrap_or(false)
    }

    fn verify(&self, high: bool) -> io::Result<()> {
        for (address, expected) in registers(high) {
            fs::write(&self.dump, format!("0x{address:08x},0x{address:08x}\n"))?;
            let raw = fs::read_to_string(&self.dump)?;
            if register_value(&raw) != Some(expected) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("0x{address:08x} did not read back as 0x{expected:08x}"),
                ));
            }
        }
        Ok(())
    }
}

fn writable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o222 != 0)
}

fn register_value(raw: &str) -> Option<u32> {
    u32::from_str_radix(raw.trim().strip_prefix("0x")?, 16).ok()
}

fn registers(high: bool) -> [(u32, u32); 2] {
    if high {
        [(DIVIDER, 0xf000_000a), (PLL, 0x8800_2b03)]
    } else {
        [(PLL, 0x8800_1703), (DIVIDER, 0xf000_000b)]
    }
}

fn apply(
    high: bool,
    mut write: impl FnMut(u32, u32) -> io::Result<()>,
    mut verify: impl FnMut(bool) -> io::Result<()>,
) -> io::Result<()> {
    let result = (|| {
        for (address, value) in registers(high) {
            write(address, value)?;
        }
        verify(high)
    })();
    if result.is_err() {
        for (address, value) in registers(false) {
            if let Err(e) = write(address, value) {
                eprintln!("slot: refresh: restoring 60 Hz at 0x{address:08x}: {e}");
            }
        }
        if let Err(e) = verify(false) {
            eprintln!("slot: refresh: verifying restored 60 Hz: {e}");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabling_lowers_the_divider_before_raising_the_pll() {
        let mut wrote = Vec::new();
        let mut verified = Vec::new();
        apply(
            true,
            |a, v| {
                wrote.push((a, v));
                Ok(())
            },
            |high| {
                verified.push(high);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(wrote, [(DIVIDER, 0xf000_000a), (PLL, 0x8800_2b03)]);
        assert_eq!(verified, [true]);
    }

    #[test]
    fn disabling_lowers_the_pll_before_raising_the_divider() {
        let mut wrote = Vec::new();
        apply(
            false,
            |a, v| {
                wrote.push((a, v));
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(wrote, [(PLL, 0x8800_1703), (DIVIDER, 0xf000_000b)]);
    }

    #[test]
    fn a_partial_enable_restores_both_normal_registers() {
        let mut wrote = Vec::new();
        let mut verified = Vec::new();
        let result = apply(
            true,
            |a, v| {
                wrote.push((a, v));
                if wrote.len() == 2 {
                    Err(io::Error::other("PLL write failed"))
                } else {
                    Ok(())
                }
            },
            |high| {
                verified.push(high);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(
            wrote,
            [
                (DIVIDER, 0xf000_000a),
                (PLL, 0x8800_2b03),
                (PLL, 0x8800_1703),
                (DIVIDER, 0xf000_000b),
            ]
        );
        assert_eq!(verified, [false]);
    }

    #[test]
    fn a_readback_mismatch_restores_and_checks_normal_refresh() {
        let mut wrote = Vec::new();
        let mut verified = Vec::new();
        let result = apply(
            true,
            |a, v| {
                wrote.push((a, v));
                Ok(())
            },
            |high| {
                verified.push(high);
                if high {
                    Err(io::Error::other("readback mismatch"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(&wrote[2..], [(PLL, 0x8800_1703), (DIVIDER, 0xf000_000b)]);
        assert_eq!(verified, [true, false]);
    }

    #[test]
    fn the_kernel_dump_contains_one_hex_value() {
        assert_eq!(register_value("0xf000000a\n"), Some(0xf000_000a));
        assert_eq!(register_value("0x88002b03\n"), Some(0x8800_2b03));
        assert_eq!(register_value("0x06511044,0x06511044\n"), None);
        assert_eq!(register_value(""), None);
    }

    #[test]
    fn rollback_still_restores_the_divider_if_restoring_the_pll_fails() {
        let mut wrote = Vec::new();
        let result = apply(
            true,
            |a, v| {
                wrote.push((a, v));
                if a == PLL {
                    Err(io::Error::other("PLL unavailable"))
                } else {
                    Ok(())
                }
            },
            |_| Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(wrote.last(), Some(&(DIVIDER, 0xf000_000b)));
    }
}
