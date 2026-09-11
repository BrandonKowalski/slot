use std::cell::Cell;
use std::ffi::{c_char, c_int, c_uint, c_void, CString};
use std::marker::PhantomData;
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

use libloading::Library;

use crate::core::{AvInfo, ButtonMask, CoreError, RetroCore, GBA_H, GBA_W};
use crate::ffi::*;
use crate::link::Link;
use crate::rumble::Rumble;

const VIDEO_BYTES: usize = (GBA_W * GBA_H * 4) as usize;

/// A libretro core keeps its emulator in dylib globals, so a second live core would share
/// the first one's machine.
static LIVE: AtomicBool = AtomicBool::new(false);

/// mGBA's libretro build is compiled `COLOR_16_BIT`, so it only ever offers RGB565 and the
/// host converts. A core built the other way needs no conversion.
#[derive(Copy, Clone, PartialEq, Eq)]
enum PixelFormat {
    Xrgb8888,
    Rgb565,
}

/// Everything the core's callbacks read or write. Lives in a `Box` so its address survives
/// the `LibretroCore` being moved.
struct Host {
    video: Vec<u8>,
    format: PixelFormat,
    audio: Vec<i16>,
    input: u16,
    system_dir: CString,
    save_dir: CString,
    rumble: Rumble,
    /// A core that is never offered the interface disables rumble outright and says nothing
    /// about it, so this is the only way to see that the offer was taken.
    asked_for_rumble: bool,
    /// The core's own netpacket vtable, handed over during `retro_load_game`. Its function
    /// pointers belong to the dylib and stay valid for as long as it is loaded. `None` until
    /// the core registers one, and legally back to `None` if the core later withdraws it.
    netpacket: Option<NetpacketCallback>,
    /// Where the core's serial traffic actually goes. Created eagerly, exactly like `rumble`
    /// above, so `net()` always hands back something valid whether or not this core ever
    /// registers netpacket.
    net: Link,
    /// The peer's own libretro client id — `1 - client_id` for the two parties this product
    /// has — set by `begin_link` from the id it was actually given, and read by
    /// `netpacket_poll_receive`/`drain_link` so every packet handed to the core is tagged
    /// with who it is really from. Without this both trampolines hardcoded client 0, so the
    /// host told the core every packet — including the joiner's — came from itself. `None`
    /// before a session starts and once `halt_link` has read it back out for `disconnected`.
    net_peer: Option<u16>,
    /// Core options, keyed as libretro names them. Values are kept as CStrings because the
    /// pointer handed back to the core has to stay valid after the callback returns.
    options: std::collections::HashMap<String, std::ffi::CString>,
    /// Set when an option changed since the core last asked, cleared when it does.
    options_dirty: bool,
}

thread_local! {
    static ACTIVE: Cell<*mut Host> = const { Cell::new(ptr::null_mut()) };
}

/// Publishes the host to the callbacks for the duration of one call into the core. The
/// lifetime keeps Rust off the `Host` while the core has it.
struct Active<'a>(PhantomData<&'a mut Host>);

impl Active<'_> {
    fn bind(host: &mut Host) -> Active<'_> {
        ACTIVE.with(|a| a.set(host as *mut Host));
        Active(PhantomData)
    }
}

impl Drop for Active<'_> {
    fn drop(&mut self) {
        ACTIVE.with(|a| a.set(ptr::null_mut()));
    }
}

/// # Safety
/// Only call from a core callback, which the core only invokes while an `Active` is bound.
unsafe fn with_host<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    let p = ACTIVE.with(|a| a.get());
    if p.is_null() {
        return None;
    }
    Some(f(&mut *p))
}

/// libretro's log callback is variadic, which stable Rust cannot express. The arity only
/// differs in the arguments a no-op never reads.
unsafe extern "C" fn log_noop(_level: c_uint, _fmt: *const c_char) {}

/// Called from the emulator thread, once per frame while a cart is buzzing.
unsafe extern "C" fn set_rumble_state(port: c_uint, effect: c_uint, strength: u16) -> bool {
    with_host(|h| h.rumble.set(port, effect, strength)).unwrap_or(false)
}

unsafe extern "C" fn environment(cmd: c_uint, data: *mut c_void) -> bool {
    match cmd {
        SET_PIXEL_FORMAT => {
            if data.is_null() {
                return false;
            }
            let want = match *(data as *const c_uint) {
                PIXEL_FORMAT_XRGB8888 => PixelFormat::Xrgb8888,
                PIXEL_FORMAT_RGB565 => PixelFormat::Rgb565,
                _ => return false,
            };
            with_host(|h| h.format = want).is_some()
        }
        GET_SYSTEM_DIRECTORY | GET_SAVE_DIRECTORY => {
            if data.is_null() {
                return false;
            }
            with_host(|h| {
                let dir = if cmd == GET_SYSTEM_DIRECTORY {
                    &h.system_dir
                } else {
                    &h.save_dir
                };
                *(data as *mut *const c_char) = dir.as_ptr();
                true
            })
            .unwrap_or(false)
        }
        GET_VARIABLE => {
            if data.is_null() {
                return false;
            }
            let var = &mut *(data as *mut Variable);
            // Every other arm here guards `data` and stops; this one goes on to deref a
            // second pointer the core packed inside it, which the null check above never
            // covers.
            if var.key.is_null() {
                return false;
            }
            let Ok(key) = std::ffi::CStr::from_ptr(var.key).to_str() else {
                return false;
            };
            with_host(|h| match h.options.get(key) {
                // The core reads this pointer after we return, so it must point at storage
                // the host owns and keeps, not at a temporary.
                Some(value) => {
                    var.value = value.as_ptr();
                    true
                }
                None => {
                    var.value = ptr::null();
                    false
                }
            })
            .unwrap_or(false)
        }
        GET_VARIABLE_UPDATE => {
            if data.is_null() {
                return false;
            }
            with_host(|h| {
                *(data as *mut bool) = h.options_dirty;
                h.options_dirty = false;
                true
            })
            .unwrap_or(false)
        }
        SET_VARIABLES => true,
        GET_RUMBLE_INTERFACE => {
            if data.is_null() {
                return false;
            }
            with_host(|h| {
                h.asked_for_rumble = true;
                (*(data as *mut RumbleInterface)).set_rumble_state = set_rumble_state;
            })
            .is_some()
        }
        GET_LOG_INTERFACE => {
            if data.is_null() {
                return false;
            }
            (*(data as *mut LogCallback)).log = log_noop as *const c_void;
            true
        }
        SET_NETPACKET_INTERFACE => {
            // A NULL pointer is the core withdrawing the interface, which is legal.
            if data.is_null() {
                return with_host(|h| h.netpacket = None).is_some();
            }
            with_host(|h| {
                h.netpacket = Some(std::ptr::read(data as *const NetpacketCallback));
            })
            .is_some()
        }
        _ => false,
    }
}

/// Called by the core, on the emulator thread, to hand us a packet to put on the wire. A C
/// function pointer cannot capture, so — like every other callback here — this reaches the
/// host through the thread-local rather than a closure.
///
/// The core only ever calls this after `cb.start` has handed it this function pointer, which
/// `begin_link` (behind `RetroCore::start_link`) is what actually does now.
unsafe extern "C" fn netpacket_send(
    _flags: c_int,
    buf: *const c_void,
    len: usize,
    _client_id: u16,
) {
    if buf.is_null() || len == 0 {
        return;
    }
    let bytes = std::slice::from_raw_parts(buf as *const u8, len).to_vec();
    with_host(|h| h.net.push_outbound(bytes));
}

/// The core asking us, mid frame, to hand it anything that has arrived. `pump_link` already
/// does this once a frame; answering here too keeps a core that polls aggressively between
/// frames from starving.
///
/// Same story as `netpacket_send`: live only once `cb.start` has been called with this
/// pointer, which `begin_link` now does.
///
/// gpSP documents `receive` as able to reenter this same thread before it returns —
/// `rfu.c:879-882`: `receive -> rfu_net_receive -> netpacket_send`, and `netpacket_send`
/// reaches the host through `with_host` too, deriving a *second* `&mut Host` from the same
/// raw pointer while this function's own borrow would still be alive if it called `receive`
/// from inside `with_host`'s closure. Miri confirms that as a Stacked Borrows violation, so
/// everything needed from the host — the function pointer, the peer's client id, and a
/// `Link` handle (`Arc`-backed, cheap to clone) — is copied out and the borrow dropped
/// before `receive` is ever called.
unsafe extern "C" fn netpacket_poll_receive() {
    let Some((receive, net, client_id)) = with_host(|h| {
        let receive = h.netpacket.as_ref().and_then(|cb| cb.receive)?;
        // No fallback to `0`: that id is the host's own, and handing a packet to the core
        // tagged with it is exactly the C2 bug (packets mislabelled as our own) this file
        // already fixed once, reopened by a different route. `net_peer` is `None` for a
        // real moment `halt_link` creates — it takes the peer out before `Cmd::EndLink`'s
        // own `link.set_active(false)` runs, so a core that reenters from its `stop`
        // callback (the same reentrancy `receive` and `start` are already proven safe
        // against) can observe `is_active() == true` with no peer recorded. `?` makes that
        // window a skipped delivery instead of a mislabelled one.
        let client_id = h.net_peer?;
        Some((receive, h.net.clone(), client_id))
    })
    .flatten() else {
        return;
    };
    // Asked on the cloned handle rather than back through `with_host`: a plain atomic load
    // on `Arc`-shared state needs no host borrow at all. A session that has already ended —
    // `Cmd::EndLink` marks this false before the transport is dropped — must not hand the
    // core a packet that arrived for a session that is no longer live; see `Link::clear` for
    // the queue's own half of that guarantee.
    if !net.is_active() {
        return;
    }
    while let Some(packet) = net.take_inbound() {
        receive(packet.as_ptr() as *const c_void, packet.len(), client_id);
    }
}

/// The logic behind `RetroCore::start_link` on `LibretroCore`, factored out to a free
/// function the same way `drain_link` is behind `pump_link` below — so it can be driven
/// directly against a bare `Host` in tests, with no dylib to open. `client_id` 0 is the
/// host, 1 the joiner, the only two this product has — so the peer is always the other one.
///
/// Marks the link active only once there is actually somewhere for `start` to have gone: a
/// core that never registered netpacket has no session to begin, and reporting one active
/// with nobody to carry it would be a lie the interlocks elsewhere would believe.
///
/// `start`, like `receive` above, is documented as able to reenter this thread, so it is
/// called with no `&mut Host` borrow alive — the same fix, the same Miri-confirmed hazard.
/// `connected` gets the identical treatment for the identical reason.
unsafe fn begin_link(client_id: u16) {
    let Some(start) = with_host(|h| h.netpacket.as_ref().and_then(|cb| cb.start)).flatten() else {
        return;
    };
    start(client_id, netpacket_send, netpacket_poll_receive);

    let peer = 1u16.wrapping_sub(client_id);
    let connected = with_host(|h| {
        h.net.set_active(true);
        h.net_peer = Some(peer);
        h.netpacket.as_ref().and_then(|cb| cb.connected)
    })
    .flatten();
    // gpSP's serial IRQ timing counts connected peers — `serial_irq_cycles = tim[...] *
    // (netplay_num_clients + 1)` (`serial.c:175`) — and `netplay_num_clients` is what this
    // call is what feeds. Never calling it left the host computing half the transfer time
    // RetroArch would. Optional and null-checked like every other netpacket callback:
    // libretro guarantees only `start`/`receive`. Its `bool` return is peer admission for a
    // session with more than two participants, which this product does not model — see
    // `NetpacketCallback`'s own doc comment — so it is read for nothing here; a handshake
    // that could actually refuse a peer is the larger design question I6 defers.
    if let Some(connected) = connected {
        connected(peer);
    }
}

/// The logic behind `RetroCore::stop_link`, mirroring `begin_link` immediately above: a free
/// function so it too can be driven directly against a bare `Host` in tests, with no dylib.
///
/// `stop` is documented OPTIONAL — libretro guarantees only `start` and `receive` — so a
/// core that never filled it in has nothing to call through, and this is silently a no-op
/// rather than something every caller has to check for first. Deliberately does not touch
/// `h.net`'s active flag: that is `Link::set_active`'s job, called by whoever is ending the
/// session (see `Cmd::EndLink` in `slot`'s `emu.rs`) whether or not the core had a `stop` to
/// hear it through — a core with no `stop` still needs its session marked over.
///
/// Calls `disconnected` first, `start`/`connected`'s counterpart — with the same peer id
/// `begin_link` derived and the same borrow-dropped-before-the-call treatment, on the
/// (unproven but cheap-to-apply) chance `stop` can reenter the same way `start` does. `None`
/// peer means `begin_link` never actually started a session, so there is no `disconnected`
/// to send — `halt_link` is safe to call whether or not one was ever begun.
unsafe fn halt_link() {
    let Some((stop, peer, disconnected)) = with_host(|h| {
        let stop = h.netpacket.as_ref().and_then(|cb| cb.stop);
        let peer = h.net_peer.take();
        let disconnected = h.netpacket.as_ref().and_then(|cb| cb.disconnected);
        (stop, peer, disconnected)
    }) else {
        return;
    };
    if let (Some(peer), Some(disconnected)) = (peer, disconnected) {
        disconnected(peer);
    }
    if let Some(stop) = stop {
        stop();
    }
}

/// The logic behind `LibretroCore::pump_link`, factored out to a free function that reaches
/// the host through the thread-local instead of `&mut self`, so it can be driven directly
/// against a bare `Host` in tests the same way the `GET_VARIABLE` tests below drive
/// `environment` — gpSP is the only core that will ever exercise this for real, and driving
/// it through a bare `Host` keeps these tests off the dylib entirely. (`task core` does
/// vendor a host gpSP build now, but a test that needs a real core is a test that cannot run
/// on a machine that has not fetched one.)
///
/// Same reentrancy hazard as `netpacket_poll_receive`, same fix: `receive` and `poll` are
/// plain `Copy` function pointers, cheap to take out of the borrow alongside the cloned
/// `Link`, so nothing here calls into the core while still holding `&mut Host`. Same
/// `is_active` guard too, for the same stale-packet reason.
unsafe fn drain_link() {
    let Some((receive, poll, net, client_id)) = with_host(|h| {
        let cb = h.netpacket.as_ref()?;
        // Same fix as `netpacket_poll_receive`, same reason: `0` is our own id, and a
        // fallback to it would mislabel the sender instead of simply not delivering.
        let client_id = h.net_peer?;
        Some((cb.receive, cb.poll, h.net.clone(), client_id))
    })
    .flatten() else {
        return;
    };
    if !net.is_active() {
        return;
    }
    if let Some(receive) = receive {
        while let Some(packet) = net.take_inbound() {
            receive(packet.as_ptr() as *const c_void, packet.len(), client_id);
        }
    }
    if let Some(poll) = poll {
        poll();
    }
}

unsafe extern "C" fn video_refresh(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    if data.is_null() {
        return; // duplicate frame, keep the previous one
    }
    with_host(|h| {
        let cols = width.min(GBA_W) as usize;
        let rows = height.min(GBA_H) as usize;
        for y in 0..rows {
            let src = (data as *const u8).add(y * pitch);
            let row = y * GBA_W as usize * 4;
            match h.format {
                PixelFormat::Xrgb8888 => {
                    ptr::copy_nonoverlapping(src, h.video.as_mut_ptr().add(row), cols * 4);
                }
                PixelFormat::Rgb565 => {
                    for x in 0..cols {
                        let p = ptr::read_unaligned((src as *const u16).add(x));
                        let r = ((p >> 11) & 0x1f) as u8;
                        let g = ((p >> 5) & 0x3f) as u8;
                        let b = (p & 0x1f) as u8;
                        let o = row + x * 4;
                        h.video[o] = (b << 3) | (b >> 2);
                        h.video[o + 1] = (g << 2) | (g >> 4);
                        h.video[o + 2] = (r << 3) | (r >> 2);
                        h.video[o + 3] = 0;
                    }
                }
            }
        }
    });
}

unsafe extern "C" fn audio_sample(left: i16, right: i16) {
    with_host(|h| h.audio.extend_from_slice(&[left, right]));
}

unsafe extern "C" fn audio_batch(data: *const i16, frames: usize) -> usize {
    if !data.is_null() {
        with_host(|h| {
            h.audio
                .extend_from_slice(std::slice::from_raw_parts(data, frames * 2))
        });
    }
    frames
}

unsafe extern "C" fn input_poll() {}

unsafe extern "C" fn input_state(port: c_uint, device: c_uint, _index: c_uint, id: c_uint) -> i16 {
    if port != 0 || device != DEVICE_JOYPAD {
        return 0;
    }
    with_host(|h| match id {
        JOYPAD_MASK => h.input as i16,
        _ if id < 16 => ((h.input >> id) & 1) as i16,
        _ => 0,
    })
    .unwrap_or(0)
}

/// One live libretro core. Which emulator this is comes from the dylib handed to `open`;
/// nothing in this type is specific to any of them. A libretro core keeps its machine in
/// dylib globals, so a second live core would share that state — `open_with` refuses to
/// open one while another is still live, rather than trusting callers not to try.
pub struct LibretroCore {
    api: Api,
    host: Box<Host>,
    /// mGBA reads the rom in place, so these bytes must outlive the loaded game.
    rom: Vec<u8>,
    rom_path: Option<CString>,
    av: AvInfo,
    loaded: bool,
    _lib: Library,
}

fn cdir(path: &Path) -> Result<CString, CoreError> {
    CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| CoreError::Load(format!("{} contains a nul", path.display())))
}

impl LibretroCore {
    /// Reports the dylib's own directory as both. That is only right when there is no
    /// content root to point at, which is every test and nothing else.
    pub fn open(dylib: &Path) -> Result<Self, CoreError> {
        let dir = dylib.parent().unwrap_or(Path::new(".")).to_path_buf();
        Self::open_with(dylib, &dir, &dir)
    }

    pub fn open_with(dylib: &Path, system_dir: &Path, save_dir: &Path) -> Result<Self, CoreError> {
        if LIVE.swap(true, Ordering::SeqCst) {
            return Err(CoreError::Unsupported("a core is already open".into()));
        }
        Self::open_inner(dylib, system_dir, save_dir)
            .inspect_err(|_| LIVE.store(false, Ordering::SeqCst))
    }

    /// What the core will search for `gba_bios.bin`. Read back from the string actually
    /// handed to the environment callback, not from the argument.
    pub fn reported_system_dir(&self) -> String {
        self.host.system_dir.to_string_lossy().into_owned()
    }

    pub fn reported_save_dir(&self) -> String {
        self.host.save_dir.to_string_lossy().into_owned()
    }

    /// Whether the core took the rumble interface, which it asks for once at init.
    pub fn asked_for_rumble(&self) -> bool {
        self.host.asked_for_rumble
    }

    /// Set a libretro core option. Takes effect the next time the core asks, which for most
    /// options means the next `retro_load_game`. That "next time" matters for aliasing, not
    /// just timing: `GET_VARIABLE` hands the core a raw pointer into the previous `CString`
    /// for this key, and calling `set_option` again for the same key drops that `CString`,
    /// freeing the memory the core's old pointer still points at. Fine for the libretro
    /// frontend contract, which only reads the pointer right after asking for it, but not
    /// safe to hold onto across a `set_option` call.
    /// Reaches `self.host` directly rather than through `with_host`. `Active::bind` is
    /// scoped to one call *into* the core, so outside such a call the thread local is null
    /// and `with_host` would silently do nothing. These are called from the frontend, never
    /// from a core callback.
    pub fn set_option(&mut self, key: &str, value: &str) {
        let Ok(value) = CString::new(value) else {
            return;
        };
        self.host.options.insert(key.to_string(), value);
        self.host.options_dirty = true;
    }

    pub fn option(&self, key: &str) -> Option<String> {
        self.host
            .options
            .get(key)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    fn open_inner(dylib: &Path, system_dir: &Path, save_dir: &Path) -> Result<Self, CoreError> {
        let lib = unsafe { Library::new(dylib) }.map_err(|e| CoreError::Load(e.to_string()))?;
        let api = unsafe { Api::load(&lib) }?;
        let version = unsafe { (api.api_version)() };
        if version != API_VERSION {
            return Err(CoreError::Unsupported(format!("libretro api {version}")));
        }
        let mut host = Box::new(Host {
            video: vec![0; VIDEO_BYTES],
            format: PixelFormat::Xrgb8888,
            audio: Vec::new(),
            input: 0,
            system_dir: cdir(system_dir)?,
            save_dir: cdir(save_dir)?,
            rumble: Rumble::default(),
            asked_for_rumble: false,
            netpacket: None,
            net: Link::default(),
            net_peer: None,
            options: std::collections::HashMap::new(),
            options_dirty: false,
        });
        unsafe {
            let _a = Active::bind(&mut host);
            (api.set_environment)(environment);
            (api.set_video_refresh)(video_refresh);
            (api.set_audio_sample)(audio_sample);
            (api.set_audio_sample_batch)(audio_batch);
            (api.set_input_poll)(input_poll);
            (api.set_input_state)(input_state);
            (api.init)();
        }
        Ok(LibretroCore {
            api,
            host,
            rom: Vec::new(),
            rom_path: None,
            av: AvInfo {
                fps: 0.0,
                sample_rate: 0.0,
            },
            loaded: false,
            _lib: lib,
        })
    }

    fn unload(&mut self) {
        if !self.loaded {
            return;
        }
        let _a = Active::bind(&mut self.host);
        unsafe { (self.api.unload_game)() };
        self.loaded = false;
    }
}

impl Drop for LibretroCore {
    fn drop(&mut self) {
        self.unload();
        unsafe {
            let _a = Active::bind(&mut self.host);
            (self.api.deinit)();
        }
        LIVE.store(false, Ordering::SeqCst);
    }
}

impl RetroCore for LibretroCore {
    fn load(&mut self, rom: &Path) -> Result<(), CoreError> {
        self.unload();
        self.rom = std::fs::read(rom)?;
        self.rom_path = Some(
            CString::new(rom.as_os_str().as_encoded_bytes())
                .map_err(|_| CoreError::Load("rom path contains a nul".into()))?,
        );
        let info = GameInfo {
            path: self.rom_path.as_ref().map_or(ptr::null(), |p| p.as_ptr()),
            data: self.rom.as_ptr() as *const c_void,
            size: self.rom.len(),
            meta: ptr::null(),
        };
        let ok = {
            let _a = Active::bind(&mut self.host);
            unsafe { (self.api.load_game)(&info) }
        };
        if !ok {
            self.rom = Vec::new();
            self.rom_path = None;
            return Err(CoreError::Load(format!("core refused {}", rom.display())));
        }
        self.loaded = true;
        let mut av = SystemAvInfo::default();
        unsafe { (self.api.get_system_av_info)(&mut av) };
        self.av = AvInfo {
            fps: av.timing.fps,
            sample_rate: av.timing.sample_rate,
        };
        unsafe { (self.api.set_controller_port_device)(0, DEVICE_JOYPAD) };
        Ok(())
    }

    fn run_frame(&mut self, input: ButtonMask) {
        if !self.loaded {
            return;
        }
        self.host.input = input.0;
        let _a = Active::bind(&mut self.host);
        unsafe { (self.api.run)() };
    }

    fn video_xrgb8888(&self) -> &[u8] {
        &self.host.video
    }

    fn take_audio(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.host.audio)
    }

    fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
        let size = unsafe { (self.api.serialize_size)() };
        if size == 0 {
            return Err(CoreError::State("core reports no state".into()));
        }
        let mut buf = vec![0u8; size];
        let ok = {
            let _a = Active::bind(&mut self.host);
            unsafe { (self.api.serialize)(buf.as_mut_ptr() as *mut c_void, size) }
        };
        if ok {
            Ok(buf)
        } else {
            Err(CoreError::State("serialize refused".into()))
        }
    }

    fn unserialize(&mut self, data: &[u8]) -> Result<(), CoreError> {
        let ok = {
            let _a = Active::bind(&mut self.host);
            unsafe { (self.api.unserialize)(data.as_ptr() as *const c_void, data.len()) }
        };
        if ok {
            Ok(())
        } else {
            Err(CoreError::State("unserialize refused".into()))
        }
    }

    fn save_ram(&self) -> Option<Vec<u8>> {
        let data = unsafe { (self.api.get_memory_data)(MEMORY_SAVE_RAM) };
        let len = unsafe { (self.api.get_memory_size)(MEMORY_SAVE_RAM) };
        if data.is_null() || len == 0 {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(data as *const u8, len) }.to_vec())
    }

    fn load_save_ram(&mut self, data: &[u8]) -> Result<(), CoreError> {
        let dst = unsafe { (self.api.get_memory_data)(MEMORY_SAVE_RAM) };
        let len = unsafe { (self.api.get_memory_size)(MEMORY_SAVE_RAM) };
        if dst.is_null() || len == 0 {
            return Err(CoreError::Unsupported("core exposes no save ram".into()));
        }
        let n = len.min(data.len());
        unsafe { ptr::copy_nonoverlapping(data.as_ptr(), dst as *mut u8, n) };
        Ok(())
    }

    fn av_info(&self) -> AvInfo {
        self.av
    }

    fn rumble(&self) -> Rumble {
        self.host.rumble.clone()
    }

    fn net(&self) -> Link {
        self.host.net.clone()
    }

    /// Begins a netpacket session: hands the core its client id and our own send/poll-receive
    /// trampolines, exactly once, the way libretro's `start` is documented to be called.
    /// `begin_link` carries the actual logic — see it for why a core that never registered
    /// netpacket leaves `net` unmarked rather than lying that a session is live.
    fn start_link(&mut self, client_id: u16) {
        let _a = Active::bind(&mut self.host);
        unsafe { begin_link(client_id) };
    }

    /// Once per frame: hand the core anything that arrived since last frame, then let it do
    /// its own polling if it offered one. Binds `Active` the same as `run_frame` does, rather
    /// than assuming `receive` and `poll` only ever touch the link — either could in
    /// principle reenter another environment callback that also reaches the host through the
    /// thread-local.
    fn pump_link(&mut self) {
        let _a = Active::bind(&mut self.host);
        unsafe { drain_link() };
    }

    /// Ends a netpacket session: tells the core it is over, if it registered a `stop` to
    /// hear it through. `halt_link` carries the actual logic — see it for why a core with no
    /// `stop` is a silent no-op rather than an error. Binds `Active` the same as
    /// `start_link`/`pump_link`, since `stop` runs on the core's own thread and may itself
    /// reach back through the thread-local.
    fn stop_link(&mut self) {
        let _a = Active::bind(&mut self.host);
        unsafe { halt_link() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::CStr;

    // `crates/slot-retro/tests/options.rs` calls `LibretroCore::set_option`/`option`, which are
    // a HashMap round trip and never reach `environment` at all — reverting the whole of the
    // `GET_VARIABLE`/`GET_VARIABLE_UPDATE` arms below left that test suite green. These
    // tests call `environment` itself, the one place a core actually crosses the ABI to ask
    // for an option, so they go red on the same revert. No dylib is needed: `environment` is
    // a plain function and a `Host` is just a struct, both reachable from inside this crate.

    fn host_with(options: HashMap<String, CString>, options_dirty: bool) -> Box<Host> {
        Box::new(Host {
            video: Vec::new(),
            format: PixelFormat::Xrgb8888,
            audio: Vec::new(),
            input: 0,
            system_dir: CString::new(".").unwrap(),
            save_dir: CString::new(".").unwrap(),
            rumble: Rumble::default(),
            asked_for_rumble: false,
            netpacket: None,
            net: Link::default(),
            net_peer: None,
            options,
            options_dirty,
        })
    }

    #[test]
    fn get_variable_writes_the_options_pointer_for_the_core_to_read() {
        let mut options = HashMap::new();
        options.insert("gpsp_serial".to_string(), CString::new("rfu").unwrap());
        let mut host = host_with(options, false);
        let _active = Active::bind(&mut host);

        let key = CString::new("gpsp_serial").unwrap();
        let mut var = Variable {
            key: key.as_ptr(),
            value: ptr::null(),
        };
        let ok = unsafe { environment(GET_VARIABLE, &mut var as *mut Variable as *mut c_void) };

        assert!(ok);
        let value = unsafe { CStr::from_ptr(var.value) };
        assert_eq!(value.to_str().unwrap(), "rfu");
    }

    #[test]
    fn get_variable_reports_false_for_an_unknown_key() {
        let mut host = host_with(HashMap::new(), false);
        let _active = Active::bind(&mut host);

        let key = CString::new("nope").unwrap();
        let mut var = Variable {
            key: key.as_ptr(),
            value: ptr::null(),
        };
        let ok = unsafe { environment(GET_VARIABLE, &mut var as *mut Variable as *mut c_void) };

        assert!(!ok);
        assert!(var.value.is_null());
    }

    /// `set_option` is what marks `options_dirty`, so the core knows to re-ask; this is the
    /// other half, that `GET_VARIABLE_UPDATE` reports it once and then clears it, matching
    /// libretro's contract that the flag means "changed since I last asked", not "changed
    /// ever".
    #[test]
    fn get_variable_update_reports_and_clears_the_dirty_flag() {
        let mut host = host_with(HashMap::new(), true);
        let _active = Active::bind(&mut host);

        let mut dirty = false;
        let ok =
            unsafe { environment(GET_VARIABLE_UPDATE, &mut dirty as *mut bool as *mut c_void) };
        assert!(ok);
        assert!(dirty, "first call must report the pending change");

        let mut dirty_again = true;
        let ok = unsafe {
            environment(
                GET_VARIABLE_UPDATE,
                &mut dirty_again as *mut bool as *mut c_void,
            )
        };
        assert!(ok);
        assert!(!dirty_again, "flag must be cleared after being read once");
    }

    /// M1: `GET_VARIABLE` derefs `var.key` past the `data` null check, so a core that hands
    /// back a `Variable` with a null key must not crash the frontend.
    #[test]
    fn get_variable_refuses_a_null_key() {
        let mut host = host_with(HashMap::new(), false);
        let _active = Active::bind(&mut host);

        let mut var = Variable {
            key: ptr::null(),
            value: ptr::null(),
        };
        let ok = unsafe { environment(GET_VARIABLE, &mut var as *mut Variable as *mut c_void) };
        assert!(!ok);
    }

    // --- netpacket -----------------------------------------------------------------------
    //
    // Same rationale as the `GET_VARIABLE` tests above: this repo has no gpSP dylib to load
    // on macOS, and gpSP is the only core that speaks netpacket at all. So the ABI seam is
    // driven directly — a hand-built `NetpacketCallback` standing in for the core, and the
    // private trampolines and `drain_link` invoked exactly as the core (or `pump_link`)
    // would invoke them.

    use std::cell::RefCell;

    thread_local! {
        static TEST_RECEIVED: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
        /// The `client_id` each `test_receive` call landed with, in the same order as
        /// `TEST_RECEIVED` — this is what proves C2: the host must stop tagging every packet
        /// with its own id and tag it with the peer's instead.
        static TEST_RECEIVED_CLIENT_IDS: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
        static TEST_POLLS: Cell<u32> = const { Cell::new(0) };
        /// What `test_start` was last called with, so `begin_link` can be proven to have
        /// actually reached the core rather than merely not panicked.
        static TEST_START_CLIENT: Cell<Option<u16>> = const { Cell::new(None) };
        /// How many times `test_stop` fired, so `halt_link` can be proven to have actually
        /// reached the core rather than merely not panicked.
        static TEST_STOP_CALLS: Cell<u32> = const { Cell::new(0) };
        /// What `test_connected` was last called with.
        static TEST_CONNECTED_CLIENT: Cell<Option<u16>> = const { Cell::new(None) };
        /// What `test_disconnected` was last called with.
        static TEST_DISCONNECTED_CLIENT: Cell<Option<u16>> = const { Cell::new(None) };
    }

    /// Thread-local recorders persist across tests that land on the same worker thread in
    /// cargo's test pool, unlike `ACTIVE`, which `Active`'s own `Drop` resets after every
    /// test. Each test that reads them must reset first.
    fn reset_test_netpacket_recorders() {
        TEST_RECEIVED.with(|r| r.borrow_mut().clear());
        TEST_RECEIVED_CLIENT_IDS.with(|c| c.borrow_mut().clear());
        TEST_POLLS.with(|p| p.set(0));
        TEST_START_CLIENT.with(|c| c.set(None));
        TEST_STOP_CALLS.with(|c| c.set(0));
        TEST_CONNECTED_CLIENT.with(|c| c.set(None));
        TEST_DISCONNECTED_CLIENT.with(|c| c.set(None));
    }

    unsafe extern "C" fn test_receive(buf: *const c_void, len: usize, client_id: u16) {
        let bytes = std::slice::from_raw_parts(buf as *const u8, len).to_vec();
        TEST_RECEIVED.with(|r| r.borrow_mut().push(bytes));
        TEST_RECEIVED_CLIENT_IDS.with(|c| c.borrow_mut().push(client_id));
    }

    /// I1: reproduces the exact reentrancy gpSP's own source documents (`rfu.c:879-882`) —
    /// `receive -> rfu_net_receive -> netpacket_send`, all on this same thread before
    /// `receive` returns. Nothing about this needs a real dylib: Miri can run entirely
    /// against these hand-built trampolines standing in for what the core does, which is
    /// what proves the fix without a gpSP binary anywhere in this tree.
    unsafe extern "C" fn test_receive_reentrant(buf: *const c_void, len: usize, client_id: u16) {
        test_receive(buf, len, client_id);
        let reentrant = b"reentrant";
        netpacket_send(0, reentrant.as_ptr() as *const c_void, reentrant.len(), 0);
    }

    /// I1's other shape: `begin_link` has the same hazard across `start(...)`, so this lets
    /// a test simulate a core that turns around and calls back into the frontend — here,
    /// `netpacket_poll_receive`, the pointer `start` was just handed — before `start` itself
    /// returns.
    unsafe extern "C" fn test_start_reentrant(
        client_id: u16,
        _send: NetpacketSend,
        poll_receive: NetpacketPollReceive,
    ) {
        TEST_START_CLIENT.with(|c| c.set(Some(client_id)));
        poll_receive();
    }

    unsafe extern "C" fn test_poll() {
        TEST_POLLS.with(|p| p.set(p.get() + 1));
    }

    unsafe extern "C" fn test_start(
        client_id: u16,
        _send: NetpacketSend,
        _poll_receive: NetpacketPollReceive,
    ) {
        TEST_START_CLIENT.with(|c| c.set(Some(client_id)));
    }

    unsafe extern "C" fn test_stop() {
        TEST_STOP_CALLS.with(|c| c.set(c.get() + 1));
    }

    unsafe extern "C" fn test_connected(client_id: u16) -> bool {
        TEST_CONNECTED_CLIENT.with(|c| c.set(Some(client_id)));
        true
    }

    unsafe extern "C" fn test_disconnected(client_id: u16) {
        TEST_DISCONNECTED_CLIENT.with(|c| c.set(Some(client_id)));
    }

    /// `start` and `receive` are the two fields libretro guarantees a core fills in; the
    /// rest are left `None`/null the way a minimal, spec-compliant core is allowed to.
    fn test_netpacket_callback() -> NetpacketCallback {
        NetpacketCallback {
            start: Some(test_start),
            receive: Some(test_receive),
            stop: None,
            poll: Some(test_poll),
            connected: None,
            disconnected: None,
            protocol_version: ptr::null(),
        }
    }

    // Every test below sets up `host.netpacket`/`host.net` *before* binding `Active`, and
    // reads them back only after that binding has dropped: `Active::bind` holds an exclusive
    // borrow of `host` for as long as the guard it returns is alive (mirroring the real
    // lifetime — the core has exclusive access to the host for the duration of one call into
    // it), so touching `host` directly while a binding is still in scope does not borrow-check.

    #[test]
    fn set_netpacket_interface_stores_the_callback_the_core_hands_over() {
        let mut host = host_with(HashMap::new(), false);
        let mut cb = test_netpacket_callback();
        let ok = {
            let _active = Active::bind(&mut host);
            unsafe {
                environment(
                    SET_NETPACKET_INTERFACE,
                    &mut cb as *mut NetpacketCallback as *mut c_void,
                )
            }
        };

        assert!(ok);
        assert!(host.netpacket.is_some(), "the callback was never stored");
    }

    #[test]
    fn set_netpacket_interface_null_data_withdraws_the_interface() {
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        let ok = {
            let _active = Active::bind(&mut host);
            unsafe { environment(SET_NETPACKET_INTERFACE, ptr::null_mut()) }
        };

        assert!(ok, "withdrawing is a legal call and must be answered true");
        assert!(
            host.netpacket.is_none(),
            "a NULL data pointer must clear a previously registered callback"
        );
    }

    #[test]
    fn netpacket_send_pushes_the_cores_packet_onto_outbound() {
        let mut host = host_with(HashMap::new(), false);
        let packet = b"link cable byte";
        {
            let _active = Active::bind(&mut host);
            unsafe {
                netpacket_send(
                    NETPACKET_RELIABLE,
                    packet.as_ptr() as *const c_void,
                    packet.len(),
                    0,
                )
            };
        }

        assert_eq!(host.net.take_outbound().as_deref(), Some(&packet[..]));
        assert_eq!(host.net.take_outbound(), None, "only one packet was sent");
    }

    #[test]
    fn netpacket_send_ignores_a_null_or_empty_packet() {
        let mut host = host_with(HashMap::new(), false);
        {
            let _active = Active::bind(&mut host);
            unsafe { netpacket_send(0, ptr::null(), 4, 0) };
            unsafe { netpacket_send(0, [1u8].as_ptr() as *const c_void, 0, 0) };
        }

        assert_eq!(
            host.net.take_outbound(),
            None,
            "a null buffer or zero length must not enqueue a phantom packet"
        );
    }

    #[test]
    fn netpacket_poll_receive_drains_inbound_into_the_cores_receive_in_order() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        host.net.set_active(true);
        // I4: a peer has to be on record for `netpacket_poll_receive` to deliver anything
        // at all now — `begin_link` sets both together in production, so this test's own
        // shortcut of activating the session directly has to set both too.
        host.net_peer = Some(1);
        host.net.push_inbound(b"first".to_vec());
        host.net.push_inbound(b"second".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { netpacket_poll_receive() };
        }

        TEST_RECEIVED.with(|r| {
            assert_eq!(
                r.borrow().as_slice(),
                &[b"first".to_vec(), b"second".to_vec()],
                "order was not kept"
            );
        });
        assert_eq!(host.net.take_inbound(), None, "queue must be drained");
    }

    #[test]
    fn netpacket_poll_receive_does_nothing_without_a_receive_callback() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        // No callback registered at all.
        host.net.set_active(true);
        host.net.push_inbound(b"stranded".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { netpacket_poll_receive() };
        }

        TEST_RECEIVED.with(|r| assert!(r.borrow().is_empty()));
        assert_eq!(
            host.net.take_inbound().as_deref(),
            Some(&b"stranded"[..]),
            "with nowhere to hand the packet it must be left queued, not dropped"
        );
    }

    /// I2: a packet that arrived after `Cmd::EndLink` marked the session inactive — but
    /// before the transport carrying it was actually dropped — must not reach a core that no
    /// longer has a session to receive it into.
    #[test]
    fn netpacket_poll_receive_does_nothing_once_the_session_is_no_longer_active() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        // Never marked active — this is what a stale packet after `Cmd::EndLink` looks like.
        host.net.push_inbound(b"stale".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { netpacket_poll_receive() };
        }

        TEST_RECEIVED.with(|r| assert!(r.borrow().is_empty(), "an ended session reached the core"));
        assert_eq!(
            host.net.take_inbound().as_deref(),
            Some(&b"stale"[..]),
            "the packet must be left queued, not delivered to a session that already ended"
        );
    }

    /// I4: `is_active()` reading true is not by itself proof a peer was ever recorded —
    /// `halt_link` takes `net_peer` back out before `Cmd::EndLink`'s own
    /// `link.set_active(false)` runs, so a core that reenters from its `stop` callback (the
    /// same shape `receive` and `start` are already proven safe against) can observe exactly
    /// this combination. The old `unwrap_or(0)` fallback would have delivered this packet
    /// mislabelled as client `0` — our own id, and the same mislabelling C2 fixed once
    /// already. Refusing to deliver at all is the point of the fix this proves.
    #[test]
    fn netpacket_poll_receive_does_nothing_without_a_recorded_peer() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        // Active with no `net_peer` — never reachable through `begin_link`, which always
        // sets both together; only a direct poke at the flag (standing in for the window
        // above) can put the host in this state at all.
        host.net.set_active(true);
        host.net.push_inbound(b"orphaned".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { netpacket_poll_receive() };
        }

        TEST_RECEIVED.with(|r| {
            assert!(
                r.borrow().is_empty(),
                "a packet must not be delivered with no peer to tag it"
            )
        });
        assert_eq!(
            host.net.take_inbound().as_deref(),
            Some(&b"orphaned"[..]),
            "the packet must be left queued, not delivered mislabelled as our own id"
        );
    }

    /// C2: the host must tag an incoming packet with the peer's client id, not its own —
    /// `begin_link` is what learns the peer's id from the one it was actually given.
    #[test]
    fn netpacket_poll_receive_tags_packets_with_the_peers_client_id() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        {
            let _active = Active::bind(&mut host);
            unsafe { begin_link(0) }; // we are the host, client 0; the peer is client 1
        }
        reset_test_netpacket_recorders(); // begin_link's own start() call is not the proof
        host.net.push_inbound(b"from the peer".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { netpacket_poll_receive() };
        }

        TEST_RECEIVED_CLIENT_IDS.with(|c| {
            assert_eq!(
                c.borrow().as_slice(),
                &[1],
                "packets must be tagged with the peer's id, not the host's own"
            );
        });
    }

    #[test]
    fn drain_link_hands_the_core_everything_waiting_then_polls() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        host.net.set_active(true);
        host.net_peer = Some(1); // I4: a peer has to be on record to deliver at all now
        host.net.push_inbound(b"queued".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { drain_link() };
        }

        TEST_RECEIVED.with(|r| assert_eq!(r.borrow().as_slice(), &[b"queued".to_vec()]));
        TEST_POLLS.with(|p| assert_eq!(p.get(), 1, "poll must run once a frame"));
    }

    #[test]
    fn drain_link_polls_even_with_nothing_inbound() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        host.net.set_active(true);
        host.net_peer = Some(1); // I4: a peer has to be on record to deliver at all now
        {
            let _active = Active::bind(&mut host);
            unsafe { drain_link() };
        }

        TEST_POLLS.with(|p| assert_eq!(p.get(), 1));
    }

    #[test]
    fn drain_link_skips_poll_when_the_core_offered_none() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(NetpacketCallback {
            poll: None,
            ..test_netpacket_callback()
        });
        host.net.set_active(true);
        host.net_peer = Some(1); // I4: a peer has to be on record to deliver at all now
        host.net.push_inbound(b"still delivered".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { drain_link() };
        }

        TEST_RECEIVED.with(|r| {
            assert_eq!(r.borrow().as_slice(), &[b"still delivered".to_vec()]);
        });
        TEST_POLLS.with(|p| assert_eq!(p.get(), 0, "poll is optional and was not offered"));
    }

    #[test]
    fn drain_link_is_a_noop_when_the_core_never_registered_netpacket() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.net.set_active(true);
        host.net.push_inbound(b"nobody asked".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { drain_link() };
        }

        TEST_RECEIVED.with(|r| assert!(r.borrow().is_empty()));
        TEST_POLLS.with(|p| assert_eq!(p.get(), 0));
        assert_eq!(
            host.net.take_inbound().as_deref(),
            Some(&b"nobody asked"[..]),
            "with no registered core the packet must be left queued, not lost"
        );
    }

    /// I2: the same stale-packet guarantee `netpacket_poll_receive` above gets, for the
    /// path `pump_link` actually drives every present.
    #[test]
    fn drain_link_does_nothing_once_the_session_is_no_longer_active() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        host.net.push_inbound(b"stale".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { drain_link() };
        }

        TEST_RECEIVED.with(|r| assert!(r.borrow().is_empty(), "an ended session reached the core"));
        TEST_POLLS.with(|p| assert_eq!(p.get(), 0, "an ended session must not be polled either"));
        assert_eq!(
            host.net.take_inbound().as_deref(),
            Some(&b"stale"[..]),
            "the packet must be left queued, not delivered to a session that already ended"
        );
    }

    /// I4: `drain_link`'s half of the same fix `netpacket_poll_receive` gets above — see that
    /// test's own doc comment for the reentrant-`stop` window this stands in for.
    #[test]
    fn drain_link_does_nothing_without_a_recorded_peer() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        host.net.set_active(true);
        host.net.push_inbound(b"orphaned".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { drain_link() };
        }

        TEST_RECEIVED.with(|r| {
            assert!(
                r.borrow().is_empty(),
                "a packet must not be delivered with no peer to tag it"
            )
        });
        TEST_POLLS.with(|p| {
            assert_eq!(
                p.get(),
                0,
                "must not even poll with no peer to tag a receive"
            )
        });
        assert_eq!(
            host.net.take_inbound().as_deref(),
            Some(&b"orphaned"[..]),
            "the packet must be left queued, not delivered mislabelled as our own id"
        );
    }

    /// C2: `drain_link`'s half of the same client-id fix `netpacket_poll_receive` gets above.
    #[test]
    fn drain_link_tags_packets_with_the_peers_client_id() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        {
            let _active = Active::bind(&mut host);
            // We are the host, client 0, so the peer is client 1 — deliberately not 0, the
            // hardcoded value C2 is about: a mutation back to that constant would otherwise
            // pass this test by coincidence whenever our own id happens to be 1.
            unsafe { begin_link(0) };
        }
        reset_test_netpacket_recorders();
        host.net.push_inbound(b"from the peer".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { drain_link() };
        }

        TEST_RECEIVED_CLIENT_IDS.with(|c| {
            assert_eq!(
                c.borrow().as_slice(),
                &[1],
                "packets must be tagged with the peer's id, not our own"
            );
        });
    }

    /// I1: `receive` reentering through `netpacket_send` — gpSP's own documented shape —
    /// must not leave a `&mut Host` borrow live across the reentrant call. This assertion is
    /// the ordinary proof (the reentrant send actually landed, so the call completed rather
    /// than being skipped); the Miri-confirmed proof is that this test runs clean at all
    /// under `cargo +nightly miri test -p slot-retro`, which a live borrow across the
    /// reentrant call reports as a Stacked Borrows violation.
    #[test]
    fn drain_link_survives_the_reentrancy_gpsp_documents() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(NetpacketCallback {
            receive: Some(test_receive_reentrant),
            ..test_netpacket_callback()
        });
        host.net.set_active(true);
        host.net_peer = Some(1); // I4: a peer has to be on record to deliver at all now
        host.net.push_inbound(b"queued".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { drain_link() };
        }

        TEST_RECEIVED.with(|r| assert_eq!(r.borrow().as_slice(), &[b"queued".to_vec()]));
        assert_eq!(
            host.net.take_outbound().as_deref(),
            Some(&b"reentrant"[..]),
            "the reentrant netpacket_send call must still reach outbound"
        );
    }

    /// `netpacket_poll_receive` is driven the exact same way `pump_link` drives `drain_link`
    /// — `RetroCore::pump_link` calls it too (see `netpacket_poll_receive`'s own doc comment)
    /// — so it carries the identical reentrancy hazard and the identical fix.
    #[test]
    fn netpacket_poll_receive_survives_the_reentrancy_gpsp_documents() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(NetpacketCallback {
            receive: Some(test_receive_reentrant),
            ..test_netpacket_callback()
        });
        host.net.set_active(true);
        host.net_peer = Some(1); // I4: a peer has to be on record to deliver at all now
        host.net.push_inbound(b"queued".to_vec());
        {
            let _active = Active::bind(&mut host);
            unsafe { netpacket_poll_receive() };
        }

        assert_eq!(host.net.take_outbound().as_deref(), Some(&b"reentrant"[..]));
    }

    // --- begin_link (the logic behind `RetroCore::start_link`) ---------------------------

    /// I1: `begin_link` has the same reentrancy hazard across `start(...)` that `drain_link`
    /// has across `receive` — a core calling back into the frontend before `start` itself
    /// returns must not find a `&mut Host` borrow still live. Proven the same way: clean
    /// under Miri, not just under an ordinary run.
    #[test]
    fn begin_link_survives_a_core_that_reenters_from_start() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(NetpacketCallback {
            start: Some(test_start_reentrant),
            ..test_netpacket_callback()
        });
        {
            let _active = Active::bind(&mut host);
            unsafe { begin_link(0) };
        }

        TEST_START_CLIENT.with(|c| assert_eq!(c.get(), Some(0)));
        assert!(host.net.is_active());
    }

    #[test]
    fn begin_link_hands_the_core_its_client_id_and_marks_the_session_active() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback());
        assert!(
            !host.net.is_active(),
            "a fresh link is not backing a session"
        );
        {
            let _active = Active::bind(&mut host);
            unsafe { begin_link(1) };
        }

        TEST_START_CLIENT.with(|c| assert_eq!(c.get(), Some(1), "client id was not forwarded"));
        assert!(
            host.net.is_active(),
            "starting a session must mark the link active"
        );
    }

    /// A core that never registered netpacket — mGBA, or gpSP before `retro_load_game` — has
    /// no `start` to call. Marking the link active anyway would tell the interlocks a session
    /// is live when nothing is carrying its traffic.
    #[test]
    fn begin_link_is_a_noop_when_the_core_never_registered_netpacket() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        {
            let _active = Active::bind(&mut host);
            unsafe { begin_link(0) };
        }

        TEST_START_CLIENT.with(|c| assert_eq!(c.get(), None, "nothing to start, nothing called"));
        assert!(!host.net.is_active());
    }

    /// I5: gpSP's serial IRQ timing counts connected peers, so a session that never calls
    /// `connected` leaves it computing half the transfer time RetroArch would.
    #[test]
    fn begin_link_calls_connected_with_the_peers_client_id() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(NetpacketCallback {
            connected: Some(test_connected),
            ..test_netpacket_callback()
        });
        {
            let _active = Active::bind(&mut host);
            unsafe { begin_link(0) }; // we are client 0; the peer is client 1
        }

        TEST_CONNECTED_CLIENT.with(|c| {
            assert_eq!(
                c.get(),
                Some(1),
                "connected was not called with the peer's id"
            )
        });
    }

    /// `connected` is documented OPTIONAL, like `stop` — a core that never offered one must
    /// not stop a session from starting, and calling through a null pointer would crash the
    /// frontend rather than the core that left it unset.
    #[test]
    fn begin_link_is_fine_with_no_connected_callback() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback()); // connected: None

        {
            let _active = Active::bind(&mut host);
            unsafe { begin_link(0) };
        }

        assert!(
            host.net.is_active(),
            "a missing connected callback must not stop the session from starting"
        );
    }

    // --- halt_link (the logic behind `RetroCore::stop_link`) -----------------------------

    #[test]
    fn halt_link_calls_the_cores_stop_when_it_registered_one() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(NetpacketCallback {
            stop: Some(test_stop),
            ..test_netpacket_callback()
        });
        {
            let _active = Active::bind(&mut host);
            unsafe { halt_link() };
        }

        TEST_STOP_CALLS.with(|c| assert_eq!(c.get(), 1, "stop was never called"));
    }

    /// `stop` is documented OPTIONAL, unlike `start` — a spec-compliant core may leave it
    /// NULL, and calling through a null pointer would crash the frontend rather than the
    /// core that never offered one.
    #[test]
    fn halt_link_is_a_noop_when_the_core_never_offered_a_stop() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(test_netpacket_callback()); // stop: None, the minimal core

        {
            let _active = Active::bind(&mut host);
            unsafe { halt_link() };
        }

        TEST_STOP_CALLS.with(|c| assert_eq!(c.get(), 0));
    }

    /// I5's other half: `disconnected` is `connected`'s counterpart, called with the same
    /// peer id `begin_link` derived when the session it was told about actually ends.
    #[test]
    fn halt_link_calls_disconnected_with_the_peers_client_id() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(NetpacketCallback {
            disconnected: Some(test_disconnected),
            ..test_netpacket_callback()
        });
        {
            let _active = Active::bind(&mut host);
            unsafe { begin_link(1) }; // we are client 1; the peer is client 0
        }
        {
            let _active = Active::bind(&mut host);
            unsafe { halt_link() };
        }

        TEST_DISCONNECTED_CLIENT.with(|c| {
            assert_eq!(
                c.get(),
                Some(0),
                "disconnected was not called with the peer's id"
            )
        });
    }

    /// `halt_link` is documented safe to call whether or not a session was ever begun — a
    /// core that never started one has no peer to report as having left.
    #[test]
    fn halt_link_does_not_call_disconnected_when_no_session_ever_started() {
        reset_test_netpacket_recorders();
        let mut host = host_with(HashMap::new(), false);
        host.netpacket = Some(NetpacketCallback {
            disconnected: Some(test_disconnected),
            ..test_netpacket_callback()
        });

        {
            let _active = Active::bind(&mut host);
            unsafe { halt_link() }; // no begin_link first
        }

        TEST_DISCONNECTED_CLIENT.with(|c| assert_eq!(c.get(), None, "nothing was ever connected"));
    }
}
