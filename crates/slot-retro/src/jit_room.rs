//! Room for gpSP's dynarec.
//!
//! gpSP's ARM64 recompiler emits direct branches between its own code and the
//! translation cache it allocates in `retro_init`, so the cache has to sit in
//! the same 256 MB window as the library. It asks for about 10.5 MB at every
//! megabyte from the library outward, and gives up after 256 tries. Giving up
//! is not checked: the cache pointer stays null and the first game it loads
//! writes to address 0x10 and crashes the whole console.
//!
//! The kernel hands libraries out from the top of the address space downward,
//! so most of the time the library lands with a whole window free below it.
//! When the process has mapped enough that the library lands within a few
//! megabytes of the bottom of a window instead, there is nowhere left for the
//! cache. That depends only on how much slot had mapped by then, which is why
//! it showed up on a cold boot and then repeated for that boot's layout.
//!
//! This looks before `retro_init` and, if the window has no room, fills it
//! with an inaccessible mapping and loads the library again, so it lands in
//! the next window down.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::Path;

pub const WINDOW: u64 = 1 << 28;
const MB: u64 = 1 << 20;
/// What gpSP asks for: ROM cache, RAM cache and the slack between them.
const CACHE: u64 = 0xa8_0000;
/// How far either side of the library it probes, in megabytes.
const REACH: i64 = 127;
#[cfg(target_os = "linux")]
const TRIES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

pub fn parse_maps(text: &str) -> Vec<(Span, String)> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let (start, end) = parts.next()?.split_once('-')?;
            let span = Span {
                start: u64::from_str_radix(start, 16).ok()?,
                end: u64::from_str_radix(end, 16).ok()?,
            };
            let path = parts.nth(4).unwrap_or("").to_string();
            Some((span, path))
        })
        .collect()
}

/// Everything the named library has mapped, as one span.
pub fn library_span(maps: &[(Span, String)], name: &str) -> Option<Span> {
    let mut found: Option<Span> = None;
    for (span, path) in maps {
        if Path::new(path).file_name().and_then(|n| n.to_str()) != Some(name) {
            continue;
        }
        found = Some(match found {
            Some(f) => Span {
                start: f.start.min(span.start),
                end: f.end.max(span.end),
            },
            None => *span,
        });
    }
    found
}

pub fn window_of(lib: Span) -> Span {
    let start = lib.start & !(WINDOW - 1);
    Span {
        start,
        end: start + WINDOW,
    }
}

/// Whether gpSP's probing, started from `lib`, can find a free spot.
pub fn has_room(maps: &[(Span, String)], lib: Span) -> bool {
    let window = window_of(lib);
    let anchor = (lib.start & !(MB - 1)) as i64;
    (-REACH..=REACH).any(|k| {
        let start = (anchor + k * MB as i64) as u64;
        let end = start + CACHE;
        start >= window.start
            && end < window.end
            && !maps.iter().any(|(s, _)| s.start < end && s.end > start)
    })
}

/// The unmapped stretches of `window`.
pub fn free_gaps(maps: &[(Span, String)], window: Span) -> Vec<Span> {
    let mut used: Vec<Span> = maps
        .iter()
        .map(|(s, _)| *s)
        .filter(|s| s.start < window.end && s.end > window.start)
        .collect();
    used.sort_by_key(|s| s.start);
    let mut gaps = Vec::new();
    let mut at = window.start;
    for s in used {
        if s.start > at {
            gaps.push(Span {
                start: at,
                end: s.start,
            });
        }
        at = at.max(s.end);
    }
    if at < window.end {
        gaps.push(Span {
            start: at,
            end: window.end,
        });
    }
    gaps
}

#[cfg(target_os = "linux")]
fn read_maps() -> Vec<(Span, String)> {
    std::fs::read_to_string("/proc/self/maps")
        .map(|t| parse_maps(&t))
        .unwrap_or_default()
}

/// Claim every free stretch of the window, so nothing is loaded into it again.
/// The address is only a hint, so a mapping that lands elsewhere is given back.
#[cfg(target_os = "linux")]
fn fill(window: Span, maps: &[(Span, String)]) {
    for gap in free_gaps(maps, window) {
        let len = (gap.end - gap.start) as usize;
        unsafe {
            let got = libc::mmap(
                gap.start as *mut libc::c_void,
                len,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
                -1,
                0,
            );
            if got == libc::MAP_FAILED {
                continue;
            }
            if got as u64 != gap.start {
                libc::munmap(got, len);
            }
        }
    }
}

/// Load the library, and again somewhere else if gpSP would have no room.
/// Never fails for want of room: if it cannot be found, gpSP is left to try.
#[cfg(target_os = "linux")]
pub fn load(dylib: &Path) -> Result<libloading::Library, libloading::Error> {
    let mut lib = unsafe { libloading::Library::new(dylib) }?;
    let Some(name) = dylib.file_name().and_then(|n| n.to_str()) else {
        return Ok(lib);
    };
    if !name.starts_with("gpsp") {
        return Ok(lib);
    }
    for attempt in 1..=TRIES {
        let maps = read_maps();
        let Some(span) = library_span(&maps, name) else {
            return Ok(lib);
        };
        if has_room(&maps, span) {
            if attempt > 1 {
                eprintln!(
                    "slot-retro: {name} loaded at {:#x} on try {attempt}, with room for its dynarec",
                    span.start
                );
            }
            return Ok(lib);
        }
        eprintln!(
            "slot-retro: {name} at {:#x} has no room for its dynarec in its 256 MB window",
            span.start
        );
        if attempt == TRIES {
            break;
        }
        drop(lib);
        fill(window_of(span), &read_maps());
        lib = unsafe { libloading::Library::new(dylib) }?;
    }
    Ok(lib)
}

#[cfg(not(target_os = "linux"))]
pub fn load(dylib: &Path) -> Result<libloading::Library, libloading::Error> {
    unsafe { libloading::Library::new(dylib) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIB: &str = "gpsp_libretro.so";

    fn maps(lines: &[&str]) -> Vec<(Span, String)> {
        parse_maps(&lines.join("\n"))
    }

    #[test]
    fn a_maps_line_gives_its_range_and_path() {
        let m = maps(&[
            "7f702b0000-7f70380000 r-xp 00000000 08:01 12 /mnt/sdcard/System/gpsp_libretro.so",
        ]);
        assert_eq!(m[0].0.start, 0x007f_702b_0000);
        assert_eq!(m[0].0.end, 0x007f_7038_0000);
        assert!(m[0].1.ends_with(LIB));
    }

    #[test]
    fn an_anonymous_line_has_no_path() {
        let m = maps(&["7f70380000-7f703a0000 rw-p 00000000 00:00 0"]);
        assert_eq!(m[0].1, "");
    }

    #[test]
    fn the_library_is_every_mapping_it_has() {
        let m = maps(&[
            "7f702b0000-7f70380000 r-xp 00000000 08:01 12 /x/gpsp_libretro.so",
            "7f70380000-7f70390000 ---p 000c8000 08:01 12 /x/gpsp_libretro.so",
            "7f70390000-7f703a0000 rw-p 000c8000 08:01 12 /x/gpsp_libretro.so",
            "7f703a0000-7f703b0000 rw-p 00000000 00:00 0",
        ]);
        let span = library_span(&m, LIB).unwrap();
        assert_eq!(span.start, 0x007f_702b_0000);
        assert_eq!(span.end, 0x007f_703a_0000);
        assert_eq!(library_span(&m, "mgba_libretro.so"), None);
    }

    /// The layout in the crash log: gpSP 2.7 MB above the bottom of its window,
    /// and everything above it already mapped.
    #[test]
    fn a_library_near_the_bottom_of_a_full_window_has_no_room() {
        let m = maps(&[
            "7f702b0000-7f70380000 r-xp 00000000 08:01 12 /x/gpsp_libretro.so",
            "7f70380000-7f7ffff000 rw-p 00000000 00:00 0",
        ]);
        let lib = library_span(&m, LIB).unwrap();
        assert!(!has_room(&m, lib));
    }

    #[test]
    fn a_library_with_a_free_window_below_it_has_room() {
        let m = maps(&[
            "7f7c800000-7f7c900000 r-xp 00000000 08:01 12 /x/gpsp_libretro.so",
            "7f7c900000-7f7ffff000 rw-p 00000000 00:00 0",
        ]);
        let lib = library_span(&m, LIB).unwrap();
        assert!(has_room(&m, lib));
    }

    #[test]
    fn free_space_above_the_library_counts_too() {
        let m = maps(&[
            "7f702b0000-7f70380000 r-xp 00000000 08:01 12 /x/gpsp_libretro.so",
            "7f71000000-7f71100000 rw-p 00000000 00:00 0",
            "7f71100000-7f7ffff000 rw-p 00000000 00:00 0",
        ]);
        let lib = library_span(&m, LIB).unwrap();
        assert!(has_room(&m, lib));
    }

    #[test]
    fn a_gap_too_small_for_the_cache_is_not_room() {
        let m = maps(&[
            "7f702b0000-7f70380000 r-xp 00000000 08:01 12 /x/gpsp_libretro.so",
            "7f70380000-7f70400000 rw-p 00000000 00:00 0",
            "7f70900000-7f7ffff000 rw-p 00000000 00:00 0",
        ]);
        let lib = library_span(&m, LIB).unwrap();
        // 0x400000..0x900000 is 5 MB, under the 10.5 MB wanted.
        assert!(!has_room(&m, lib));
    }

    #[test]
    fn the_free_gaps_of_a_window_leave_out_what_is_mapped() {
        let m = maps(&[
            "10100000-10200000 r-xp 00000000 08:01 12 /x/a.so",
            "10300000-10400000 rw-p 00000000 00:00 0",
        ]);
        let window = Span {
            start: 0x1000_0000,
            end: 0x2000_0000,
        };
        assert_eq!(
            free_gaps(&m, window),
            vec![
                Span {
                    start: 0x1000_0000,
                    end: 0x1010_0000
                },
                Span {
                    start: 0x1020_0000,
                    end: 0x1030_0000
                },
                Span {
                    start: 0x1040_0000,
                    end: 0x2000_0000
                },
            ]
        );
    }
}
