use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use slot_input::{Btn, InputSource, Millis, RawEvent};

use super::evdev::{
    decode, device_name, pick_devices, to_raw, Ev, Hat, EVENT_BYTES, EV_ABS, EV_SYN,
};
use super::trace;

const DEV: &str = "/dev/input";
const SYS: &str = "/sys/class/input";
/// Where the power supplies live, which is where this board keeps its lid.
const PSY: &str = "/sys/class/power_supply";

/// How often the lid is read. It is not an input device on this hardware — no node
/// advertises `SW_LID`, and the hall sensor is a write-only-ish attribute on the PMIC — so
/// the only way to see it is to look. Reading that attribute costs an i2c transaction to the
/// PMIC, so it is looked at a few times a second rather than every frame; a lid is a hinge,
/// and nobody can perceive 150 ms of it.
const LID_POLL_MS: Millis = 150;

/// Every node worth reading, each on a thread parked in `read`. Polling them instead would
/// mean picking a period and wearing the latency of it, and the kernel already knows the
/// moment a button moved.
pub struct DeviceInput {
    pending: Arc<Mutex<Vec<RawEvent>>>,
    /// The PMIC attribute the hall sensor reports through, if this board has one. `None` on
    /// a board that does not, which is not a failure: it is a device without a lid.
    hall: Option<PathBuf>,
    /// What the lid was last seen doing. `None` until the first read, so the first look
    /// always reports — a device that booted with the lid shut should know it.
    lid_shut: Option<bool>,
    next_lid_poll: Millis,
}

impl DeviceInput {
    /// `root` is the card, and only so a trace has somewhere to be written where it can be
    /// read back. Nothing else here touches it.
    pub fn open(root: &Path) -> Self {
        DeviceInput::open_in(Path::new(DEV), Path::new(SYS), root, trace::enabled())
    }

    /// `trace` is passed rather than read from the environment here, so a test can ask for one
    /// without racing every other test in the process for a variable they all share.
    pub fn open_in(dev: &Path, sys: &Path, root: &Path, trace: bool) -> Self {
        let trace = trace.then(|| Trace::start(root, dev, sys)).flatten();
        DeviceInput::open_traced(dev, sys, trace)
    }

    fn open_traced(dev: &Path, sys: &Path, trace: Option<Arc<Trace>>) -> Self {
        let pending: Arc<Mutex<Vec<RawEvent>>> = Arc::new(Mutex::new(Vec::new()));
        for node in pick_devices(dev, sys) {
            eprintln!(
                "slot: input {} ({})",
                node.display(),
                device_name(sys, &node)
            );
            let queue = pending.clone();
            let name = node.display().to_string();
            let trace = trace.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("slot-input-{name}"))
                .spawn(move || read_node(&node, &queue, trace.as_deref()));
            if let Err(e) = spawned {
                eprintln!("slot: input {name}: {e}");
            }
        }
        DeviceInput {
            pending,
            hall: find_hall(Path::new(PSY)),
            lid_shut: None,
            next_lid_poll: 0,
        }
    }
}

/// Until the node goes away. The threads are never joined: the process ends by powering the
/// device off, and a reader blocked on a button nobody pressed has nothing to unwind.
fn read_node(node: &Path, queue: &Mutex<Vec<RawEvent>>, trace: Option<&Trace>) {
    let label = node
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| node.display().to_string());
    // One per node: the axes are that node's, and a second pad would have its own.
    let mut hat = Hat::default();
    let mut file = match File::open(node) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("slot: input {}: {e}", node.display());
            return;
        }
    };
    // Whole events only: the driver refuses a read shorter than one and never splits them.
    let mut buf = [0u8; EVENT_BYTES * 16];
    loop {
        let read = match file.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("slot: input {}: {e}", node.display());
                return;
            }
        };
        // Traced before the mapping rather than after it, because the codes worth seeing are
        // exactly the ones `to_raw` has no button for.
        let mut events = Vec::new();
        for ev in buf[..read].chunks_exact(EVENT_BYTES).filter_map(decode) {
            if let Some(trace) = trace {
                trace.event(&label, ev);
            }
            match ev.kind {
                // The d-pad, which needs the axis it arrived on to say what it released.
                EV_ABS => events.extend(hat.feed(ev)),
                _ => events.extend(to_raw(ev)),
            }
        }
        if events.is_empty() {
            continue;
        }
        queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(events);
    }
}

/// The trace file and the clock its lines are stamped against, shared by every reader thread
/// so the lines interleave in the order the kernel produced them.
struct Trace {
    out: Mutex<File>,
    began: Instant,
}

impl Trace {
    /// `None` unless asked for, and `None` again if the card will not take the file, which is
    /// a diagnostic that failed rather than a reason not to boot. The survey goes in first:
    /// a button that never appears below is explained by a node that was skipped up here.
    fn start(root: &Path, dev: &Path, sys: &Path) -> Option<Arc<Trace>> {
        let path = root.join(trace::TRACE_FILE);
        let mut file = match File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("slot: {}: {e}", path.display());
                return None;
            }
        };
        for line in trace::survey(dev, sys) {
            let _ = writeln!(file, "{line}");
        }
        let _ = writeln!(file, "--");
        let _ = file.flush();
        Some(Arc::new(Trace {
            out: Mutex::new(file),
            began: Instant::now(),
        }))
    }

    /// Flushed a line at a time. The device is turned off by holding a button rather than by
    /// unwinding, so an edge left in a buffer is the one that was being chased.
    fn event(&self, node: &str, ev: Ev) {
        // The frame marker after every edge, which would be most of the file and none of the
        // answer.
        if ev.kind == EV_SYN {
            return;
        }
        let line = trace::event_line(node, self.began.elapsed().as_millis(), ev);
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

impl DeviceInput {
    /// `1` is open and `0` is shut, measured on an RG SP by watching the value while working
    /// the hinge. `None` where the attribute is missing or will not parse, which leaves the
    /// lid where it was rather than inventing an edge.
    fn read_lid(&self) -> Option<bool> {
        let raw = std::fs::read_to_string(self.hall.as_ref()?).ok()?;
        match raw.trim() {
            "0" => Some(true),
            "1" => Some(false),
            _ => None,
        }
    }
}

impl InputSource for DeviceInput {
    fn poll(&mut self, now: Millis) -> Vec<RawEvent> {
        let mut out = std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()));
        if self.hall.is_some() && now >= self.next_lid_poll {
            self.next_lid_poll = now + LID_POLL_MS;
            if let Some(shut) = self.read_lid() {
                if self.lid_shut != Some(shut) {
                    self.lid_shut = Some(shut);
                    // The same events an `SW_LID` node would have produced, so everything
                    // above this is unaware that this board reports its hinge through the
                    // power supply.
                    out.push(if shut {
                        RawEvent::Down(Btn::Lid)
                    } else {
                        RawEvent::Up(Btn::Lid)
                    });
                }
            }
        }
        out
    }
}

/// The hall sensor is not an input device here: no node advertises `SW_LID`, and the hinge
/// is reported by the PMIC's battery node as `hallkey`. Found by looking rather than
/// hardcoded, so a board that names it differently reads as a device without a lid instead
/// of one whose lid never moves.
fn find_hall(psy: &Path) -> Option<PathBuf> {
    let mut supplies: Vec<PathBuf> = std::fs::read_dir(psy)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    supplies.sort();
    supplies
        .into_iter()
        .map(|d| d.join("hallkey"))
        .find(|p| p.is_file())
}
