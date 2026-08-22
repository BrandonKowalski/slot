use std::cell::Cell;
use std::ffi::{c_char, c_uint, c_void, CString};
use std::marker::PhantomData;
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

use libloading::Library;

use crate::core::{AvInfo, ButtonMask, CoreError, RetroCore, GBA_H, GBA_W};
use crate::ffi::*;
use crate::rumble::Rumble;

const VIDEO_BYTES: usize = (GBA_W * GBA_H * 4) as usize;

/// A libretro core keeps its emulator in dylib globals, so a second live core would share
/// the first one's machine.
static LIVE: AtomicBool = AtomicBool::new(false);

/// The buildbot core is compiled `COLOR_16_BIT`, so it only ever offers RGB565 and the
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
        _ => false,
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
}
