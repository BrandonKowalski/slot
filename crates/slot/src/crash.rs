use std::ffi::{c_void, CStr};
use std::fmt::Write;

const SIGNALS: [(i32, &str); 5] = [
    (libc::SIGSEGV, "SIGSEGV"),
    (libc::SIGBUS, "SIGBUS"),
    (libc::SIGILL, "SIGILL"),
    (libc::SIGFPE, "SIGFPE"),
    (libc::SIGABRT, "SIGABRT"),
];

pub fn install() {
    for (sig, _) in SIGNALS {
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_crash as *const () as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK | libc::SA_RESETHAND;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, std::ptr::null_mut());
        }
    }
}

struct Line {
    buf: [u8; 1024],
    len: usize,
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

extern "C" fn on_crash(sig: i32, info: *mut libc::siginfo_t, ctx: *mut c_void) {
    let mut line = Line {
        buf: [0; 1024],
        len: 0,
    };
    let name = SIGNALS
        .iter()
        .find(|(s, _)| *s == sig)
        .map_or("signal", |(_, n)| n);
    let addr = unsafe { fault_address(info) };
    let mut thread = [0 as libc::c_char; 32];
    unsafe {
        libc::pthread_getname_np(libc::pthread_self(), thread.as_mut_ptr(), thread.len());
    }
    let thread = unsafe { CStr::from_ptr(thread.as_ptr()) }.to_string_lossy();
    let _ = write!(
        line,
        "slot: crash: {name} in thread {thread} addr {addr:#x} "
    );
    let (pc, lr) = unsafe { registers(ctx) };
    let _ = write!(line, "pc ");
    locate(&mut line, pc);
    if let Some(lr) = lr {
        let _ = write!(line, " lr ");
        locate(&mut line, lr);
    }
    let _ = writeln!(line);
    unsafe {
        libc::write(2, line.buf.as_ptr() as *const c_void, line.len);
        libc::raise(sig);
    }
}

fn locate(line: &mut Line, addr: usize) {
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    if addr == 0 || unsafe { libc::dladdr(addr as *const c_void, &mut info) } == 0 {
        let _ = write!(line, "{addr:#x}");
        return;
    }
    let file = if info.dli_fname.is_null() {
        "?".into()
    } else {
        unsafe { CStr::from_ptr(info.dli_fname) }.to_string_lossy()
    };
    let _ = write!(
        line,
        "{addr:#x} {file}+{:#x}",
        addr.wrapping_sub(info.dli_fbase as usize)
    );
    if !info.dli_sname.is_null() {
        let sym = unsafe { CStr::from_ptr(info.dli_sname) }.to_string_lossy();
        let _ = write!(
            line,
            " ({sym}+{:#x})",
            addr.wrapping_sub(info.dli_saddr as usize)
        );
    }
}

#[cfg(target_os = "linux")]
unsafe fn fault_address(info: *mut libc::siginfo_t) -> usize {
    (*info).si_addr() as usize
}

#[cfg(target_os = "macos")]
unsafe fn fault_address(info: *mut libc::siginfo_t) -> usize {
    (*info).si_addr as usize
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
unsafe fn registers(ctx: *mut c_void) -> (usize, Option<usize>) {
    let mc = &(*(ctx as *mut libc::ucontext_t)).uc_mcontext;
    (mc.pc as usize, Some(mc.regs[30] as usize))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe fn registers(ctx: *mut c_void) -> (usize, Option<usize>) {
    let mc = &(*(ctx as *mut libc::ucontext_t)).uc_mcontext;
    (mc.gregs[libc::REG_RIP as usize] as usize, None)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe fn registers(ctx: *mut c_void) -> (usize, Option<usize>) {
    let ss = &(*(*(ctx as *mut libc::ucontext_t)).uc_mcontext).__ss;
    (ss.__pc as usize, Some(ss.__lr as usize))
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
unsafe fn registers(ctx: *mut c_void) -> (usize, Option<usize>) {
    let ss = &(*(*(ctx as *mut libc::ucontext_t)).uc_mcontext).__ss;
    (ss.__rip as usize, None)
}
