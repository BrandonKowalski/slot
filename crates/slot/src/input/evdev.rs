//! The evdev wire format and the codes the device's buttons arrive as. No I/O beyond
//! reading the sysfs tree that says which node is which, so all of it is testable off
//! device.

use std::path::{Path, PathBuf};

use slot_input::{Btn, RawEvent};

/// `struct input_event`: a 64 bit timeval, then type, code and value. The kernel writes
/// these in the machine's own endianness; every target slot builds for is little endian, and
/// pinning that is what lets the decode be tested away from the device.
pub const EVENT_BYTES: usize = 24;

/// The frame marker the kernel ends every group of edges with.
pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_SW: u16 = 0x05;
pub const SW_LID: u16 = 0x00;

pub const EV_ABS: u16 = 0x03;
pub const ABS_HAT0X: u16 = 0x10;
pub const ABS_HAT0Y: u16 = 0x11;

/// The d-pad, which is two axes on hat 0 rather than four keys. A zero releases whichever
/// direction the axis was last at, and only the axis knows which that was, so this is the one
/// part of the decode that has to remember anything.
#[derive(Default)]
pub struct Hat {
    x: i32,
    y: i32,
}

impl Hat {
    /// Up to two events. A thumb rolled around the pivot swings the axis end to end without
    /// stopping in the middle, and the direction being left has to be released or it stays
    /// down for as long as the device is on.
    pub fn feed(&mut self, ev: Ev) -> Vec<RawEvent> {
        let (held, ends) = match ev.code {
            ABS_HAT0X => (&mut self.x, [Btn::Left, Btn::Right]),
            ABS_HAT0Y => (&mut self.y, [Btn::Up, Btn::Down]),
            _ => return Vec::new(),
        };
        let (was, now) = (*held, ev.value.signum());
        if now == was {
            return Vec::new();
        }
        *held = now;
        let btn = |v: i32| ends[usize::from(v > 0)];
        let mut out = Vec::new();
        if was != 0 {
            out.push(RawEvent::Up(btn(was)));
        }
        if now != 0 {
            out.push(RawEvent::Down(btn(now)));
        }
        out
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Ev {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

pub fn decode(bytes: &[u8]) -> Option<Ev> {
    let packet: &[u8; EVENT_BYTES] = bytes.get(..EVENT_BYTES)?.try_into().ok()?;
    Some(Ev {
        kind: u16::from_le_bytes([packet[16], packet[17]]),
        code: u16::from_le_bytes([packet[18], packet[19]]),
        value: i32::from_le_bytes([packet[20], packet[21], packet[22], packet[23]]),
    })
}

pub fn to_raw(ev: Ev) -> Option<RawEvent> {
    let btn = match ev.kind {
        EV_KEY => code_to_btn(ev.code)?,
        // The hinge is a switch, and its "code" collides with a key the pad may also send.
        EV_SW if ev.code == SW_LID => Btn::Lid,
        _ => return None,
    };
    match ev.value {
        0 => Some(RawEvent::Up(btn)),
        1 => Some(RawEvent::Down(btn)),
        // 2 is autorepeat: presses with no release, which would re-arm every hold and double
        // tap window in the gesture layer.
        _ => None,
    }
}

/// Read off the device with `SLOT_TRACE_INPUT`, pressing the buttons in a known order. The
/// board does not use the kernel's names for its own layout — `0x137` is `BTN_TR` in the
/// headers and START on the case — so these are transcribed rather than reasoned about. The
/// directions are absent because they are not keys here at all; see `Hat`.
pub fn code_to_btn(code: u16) -> Option<Btn> {
    Some(match code {
        0x130 => Btn::A,
        0x131 => Btn::B,
        0x132 => Btn::Y,
        0x133 => Btn::X,
        0x134 => Btn::L1,
        0x135 => Btn::R1,
        0x136 => Btn::Select,
        0x137 => Btn::Start,
        0x138 => Btn::Menu,
        0x13a => Btn::L2,
        0x13b => Btn::R2,
        115 => Btn::VolUp,
        114 => Btn::VolDown,
        116 => Btn::Power,
        // 0x162 arrives immediately behind 0x138 on one press of MENU. Mapping it too would
        // make every menu press a double tap, which is the gesture the state switcher opens
        // on, so it is deliberately nothing.
        _ => return None,
    })
}

/// Every key code the frontend has a use for. A node reporting none of them is not this
/// device's button pad, whatever it is called. The power key earns its place even though it is
/// the only thing its own node reports: left out, that whole node is passed over and the
/// button is dead however well it is mapped.
const WANTED: [u16; 15] = [
    0x130, 0x131, 0x132, 0x133, 0x134, 0x135, 0x136, 0x137, 0x138, 0x13a, 0x13b, 0x162, 115, 114,
    116,
];

/// The capability bitmask parse lives with the motor that also needs it, so the awkward part
/// — that the words are printed most significant first, and an offset only comes out right
/// counted from the end — has one definition and one set of tests.
pub use slot_power::has_bit;

/// The event nodes worth opening, in name order. Picked by what each one reports it can
/// send, never by position: `event0` is the power key on one boot and the pad on the next.
pub fn pick_devices(dev: &Path, sys: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dev) else {
        return Vec::new();
    };
    let mut nodes: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("event"))
        })
        .collect();
    nodes.sort();
    nodes.retain(|node| {
        let Some(name) = node.file_name() else {
            return false;
        };
        wanted_node(&sys.join(name).join("device"))
    });
    nodes
}

fn wanted_node(device: &Path) -> bool {
    let cap = |file: &str| std::fs::read_to_string(device.join("capabilities").join(file));
    let keys = cap("key").unwrap_or_default();
    let lid = cap("sw").is_ok_and(|sw| has_bit(&sw, SW_LID));
    lid || WANTED.iter().any(|bit| has_bit(&keys, *bit))
}

/// What sysfs calls the node, for the one line the frontend prints about what it opened.
pub fn device_name(sys: &Path, node: &Path) -> String {
    node.file_name()
        .and_then(|n| std::fs::read_to_string(sys.join(n).join("device/name")).ok())
        .map(|n| n.trim().to_string())
        .unwrap_or_else(|| node.display().to_string())
}
