//! Intel CPU temperatures through the PawnIO driver.
//!
//! Core temperatures live in model-specific registers that only kernel code
//! can read. PawnIO (<https://pawnio.eu>) is a signed driver that runs small
//! signed modules on behalf of user programs; LibreHardwareMonitor uses the
//! same `IntelMSR` module bundled here. Opening PawnIO requires an elevated
//! process, so without Administrator rights this source stays unavailable.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{FreeLibrary, HANDLE, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::SystemInformation::GROUP_AFFINITY;
use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadGroupAffinity};

use crate::windows_telemetry::{CpuTemperatures, intel_dts_celsius};

/// Signed `IntelMSR` module from namazso/PawnIO.Modules.
const INTEL_MSR_MODULE: &[u8] = include_bytes!("../assets/pawnio/IntelMSR.bin");

const MSR_TEMPERATURE_TARGET: u64 = 0x1A2;
const IA32_THERM_STATUS: u64 = 0x19C;
const IA32_PACKAGE_THERM_STATUS: u64 = 0x1B1;

type OpenFn = unsafe extern "system" fn(*mut HANDLE) -> i32;
type LoadFn = unsafe extern "system" fn(HANDLE, *const u8, usize) -> i32;
type ExecuteFn = unsafe extern "system" fn(
    HANDLE,
    *const u8,
    *const u64,
    usize,
    *mut u64,
    usize,
    *mut usize,
) -> i32;
type CloseFn = unsafe extern "system" fn(HANDLE) -> i32;
/// `FARPROC` as returned by `GetProcAddress`.
type RawProc = unsafe extern "system" fn() -> isize;

/// An open PawnIO executor with one module loaded.
struct PawnIo {
    library: HMODULE,
    handle: HANDLE,
    execute: ExecuteFn,
    close: CloseFn,
}

// SAFETY: the executor handle is not thread-affine; access goes through
// `&self` on the collector thread.
unsafe impl Send for PawnIo {}

impl PawnIo {
    fn open(module: &[u8]) -> Option<Self> {
        let library = load_pawnio_library()?;
        let symbol = |name: &[u8]| {
            // SAFETY: `library` is loaded and `name` is NUL-terminated.
            unsafe { GetProcAddress(library, name.as_ptr().cast()) }
        };
        let (Some(open), Some(load), Some(execute), Some(close)) = (
            symbol(b"pawnio_open\0"),
            symbol(b"pawnio_load\0"),
            symbol(b"pawnio_execute\0"),
            symbol(b"pawnio_close\0"),
        ) else {
            // SAFETY: loaded above and not used afterwards.
            unsafe { FreeLibrary(library) };
            return None;
        };
        // SAFETY: the exports have the signatures declared in PawnIOLib.h.
        let (open, load, execute, close) = unsafe {
            (
                std::mem::transmute::<RawProc, OpenFn>(open),
                std::mem::transmute::<RawProc, LoadFn>(load),
                std::mem::transmute::<RawProc, ExecuteFn>(execute),
                std::mem::transmute::<RawProc, CloseFn>(close),
            )
        };
        let mut handle: HANDLE = null_mut();
        // SAFETY: valid out-pointer. Fails with E_ACCESSDENIED unless elevated.
        if unsafe { open(&mut handle) } < 0 {
            // SAFETY: loaded above and not used afterwards.
            unsafe { FreeLibrary(library) };
            return None;
        }
        let io = Self {
            library,
            handle,
            execute,
            close,
        };
        // SAFETY: `module` outlives the call.
        (unsafe { load(handle, module.as_ptr(), module.len()) } >= 0).then_some(io)
    }

    /// Read an MSR on the CPU the calling thread is running on.
    fn read_msr(&self, msr: u64) -> Option<u64> {
        let input = [msr];
        let mut output = [0u64; 1];
        let mut written = 0usize;
        // SAFETY: buffers match the counts passed; the name is NUL-terminated.
        let status = unsafe {
            (self.execute)(
                self.handle,
                c"ioctl_read_msr".as_ptr().cast(),
                input.as_ptr(),
                input.len(),
                output.as_mut_ptr(),
                output.len(),
                &mut written,
            )
        };
        (status >= 0 && written == 1).then_some(output[0])
    }
}

impl Drop for PawnIo {
    fn drop(&mut self) {
        // SAFETY: closes the executor opened in `open`, then unloads the DLL.
        unsafe {
            (self.close)(self.handle);
            FreeLibrary(self.library);
        }
    }
}

fn load_pawnio_library() -> Option<HMODULE> {
    let installed = std::env::var_os("ProgramFiles")
        .map(|dir| std::path::Path::new(&dir).join(r"PawnIO\PawnIOLib.dll"));
    installed
        .into_iter()
        .chain(Some("PawnIOLib.dll".into()))
        .find_map(|path| {
            use std::os::windows::ffi::OsStrExt;
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: `wide` is NUL-terminated.
            let library = unsafe { LoadLibraryW(wide.as_ptr()) };
            (!library.is_null()).then_some(library)
        })
}

/// Run `f` pinned to the first logical CPU of `core`, then restore the
/// thread's previous affinity.
fn on_core<T>(core: &GROUP_AFFINITY, f: impl FnOnce() -> T) -> Option<T> {
    let target = GROUP_AFFINITY {
        Mask: core.Mask & core.Mask.wrapping_neg(),
        Group: core.Group,
        Reserved: [0; 3],
    };
    // SAFETY: all-zero is a valid GROUP_AFFINITY out-buffer.
    let mut previous: GROUP_AFFINITY = unsafe { std::mem::zeroed() };
    // SAFETY: pseudo-handle for the current thread; valid in/out pointers.
    if unsafe { SetThreadGroupAffinity(GetCurrentThread(), &target, &mut previous) } == 0 {
        return None;
    }
    let value = f();
    // SAFETY: restores the affinity saved above.
    unsafe { SetThreadGroupAffinity(GetCurrentThread(), &previous, null_mut()) };
    Some(value)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn intel_dts_support() -> Option<bool> {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::__cpuid;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::__cpuid;

    #[allow(unused_unsafe)]
    // SAFETY: CPUID is available on every x86 CPU Windows supports.
    let (vendor, thermal) = unsafe { (__cpuid(0), __cpuid(6)) };
    let is_intel = (vendor.ebx, vendor.edx, vendor.ecx) == (0x756E_6547, 0x4965_6E69, 0x6C65_746E);
    // CPUID.06H:EAX bit 0 = digital thermal sensor, bit 6 = package sensor.
    (is_intel && vendor.eax >= 6 && thermal.eax & 1 != 0).then_some(thermal.eax & (1 << 6) != 0)
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn intel_dts_support() -> Option<bool> {
    None
}

/// Per-core and package digital thermal sensors on Intel CPUs.
pub struct IntelDts {
    io: PawnIo,
    /// Each physical core's affinity and its MSR_TEMPERATURE_TARGET.
    cores: Vec<(GROUP_AFFINITY, u64)>,
    has_package_sensor: bool,
}

impl IntelDts {
    /// `cores` are the physical cores in OS order. Returns `None` on
    /// non-Intel CPUs, when PawnIO isn't installed, or when not elevated.
    pub fn open(cores: &[GROUP_AFFINITY]) -> Option<Self> {
        let has_package_sensor = intel_dts_support()?;
        let io = PawnIo::open(INTEL_MSR_MODULE)?;
        let cores = cores
            .iter()
            .map(|core| {
                let target = on_core(core, || io.read_msr(MSR_TEMPERATURE_TARGET)).flatten();
                // Pre-Nehalem parts lack the MSR; 100 °C is the usual TjMax.
                (*core, target.unwrap_or(100 << 16))
            })
            .collect();
        Some(Self {
            io,
            cores,
            has_package_sensor,
        })
    }

    pub fn sample(&self) -> Option<CpuTemperatures> {
        let cores: Vec<f32> = self
            .cores
            .iter()
            .map(|(core, target)| {
                on_core(core, || self.io.read_msr(IA32_THERM_STATUS))
                    .flatten()
                    .and_then(|status| intel_dts_celsius(*target, status, true))
            })
            .collect::<Option<_>>()?;
        let package = self
            .has_package_sensor
            .then(|| {
                let (core, target) = self.cores.first()?;
                let status = on_core(core, || self.io.read_msr(IA32_PACKAGE_THERM_STATUS))??;
                intel_dts_celsius(*target, status, false)
            })
            .flatten()
            .or_else(|| cores.iter().copied().reduce(f32::max));
        Some(CpuTemperatures { package, cores })
    }
}
