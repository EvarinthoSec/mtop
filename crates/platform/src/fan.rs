use mtop_core::model::{FanSnapshot, FanStatus, FanTelemetry};

pub trait FanProvider: Send {
    fn collect(&mut self) -> FanTelemetry;
}

#[derive(Default)]
pub struct NoopFanProvider;

impl FanProvider for NoopFanProvider {
    fn collect(&mut self) -> FanTelemetry {
        FanTelemetry::default()
    }
}

pub fn fan_provider_for() -> Box<dyn FanProvider> {
    #[cfg(target_os = "macos")]
    {
        return Box::new(macos::MacSmcFanProvider::new());
    }
    #[allow(unreachable_code)]
    Box::new(NoopFanProvider)
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{ffi::c_void, mem::size_of};

    use super::*;

    type MachPort = u32;
    type KernReturn = i32;
    const SMC_SELECTOR: u32 = 2;
    const READ_BYTES: u8 = 5;
    const READ_KEY_INFO: u8 = 9;
    const KIO_NOT_PRIVILEGED: KernReturn = 0xE00002C1u32 as i32;
    const KEY_NOT_FOUND: u8 = 0x84;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct SmcVersion {
        major: u8,
        minor: u8,
        build: u8,
        reserved: u8,
        release: u16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct SmcPLimitData {
        version: u16,
        length: u16,
        cpu_plimit: u32,
        gpu_plimit: u32,
        mem_plimit: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct SmcKeyInfo {
        data_size: u32,
        data_type: u32,
        attributes: u8,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct SmcKeyData {
        key: u32,
        version: SmcVersion,
        p_limit: SmcPLimitData,
        key_info: SmcKeyInfo,
        result: u8,
        status: u8,
        command: u8,
        data32: u32,
        bytes: [u8; 32],
    }

    const _: () = assert!(size_of::<SmcKeyData>() == 80);

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOServiceMatching(name: *const u8) -> *mut c_void;
        fn IOServiceGetMatchingService(port: MachPort, matching: *mut c_void) -> MachPort;
        fn IOServiceOpen(
            service: MachPort,
            task: MachPort,
            kind: u32,
            connection: *mut MachPort,
        ) -> KernReturn;
        fn IOServiceClose(connection: MachPort) -> KernReturn;
        fn IOObjectRelease(object: MachPort) -> KernReturn;
        fn IOConnectCallStructMethod(
            connection: MachPort,
            selector: u32,
            input: *const c_void,
            input_size: usize,
            output: *mut c_void,
            output_size: *mut usize,
        ) -> KernReturn;
        static mach_task_self_: MachPort;
    }

    #[derive(Clone, Copy)]
    enum ReadError {
        NotRoot,
        Missing,
        Failed,
    }

    struct Smc(MachPort);

    impl Smc {
        fn open() -> Result<Self, ReadError> {
            let matching = unsafe { IOServiceMatching(c"AppleSMC".as_ptr().cast()) };
            if matching.is_null() {
                return Err(ReadError::Failed);
            }
            let service = unsafe { IOServiceGetMatchingService(0, matching) };
            if service == 0 {
                return Err(ReadError::Failed);
            }
            let mut connection = 0;
            let result = unsafe { IOServiceOpen(service, mach_task_self_, 0, &mut connection) };
            unsafe { IOObjectRelease(service) };
            match result {
                0 => Ok(Self(connection)),
                KIO_NOT_PRIVILEGED => Err(ReadError::NotRoot),
                _ => Err(ReadError::Failed),
            }
        }

        fn call(&self, request: &SmcKeyData) -> Result<SmcKeyData, ReadError> {
            let mut response = SmcKeyData::default();
            let mut output_size = size_of::<SmcKeyData>();
            let result = unsafe {
                IOConnectCallStructMethod(
                    self.0,
                    SMC_SELECTOR,
                    (request as *const SmcKeyData).cast(),
                    size_of::<SmcKeyData>(),
                    (&mut response as *mut SmcKeyData).cast(),
                    &mut output_size,
                )
            };
            match result {
                KIO_NOT_PRIVILEGED => Err(ReadError::NotRoot),
                0 if response.result == 0 => Ok(response),
                0 if response.result == KEY_NOT_FOUND => Err(ReadError::Missing),
                _ => Err(ReadError::Failed),
            }
        }

        fn read(&self, key_name: &str) -> Result<(u32, [u8; 32]), ReadError> {
            let key = key_name.as_bytes();
            if key.len() != 4 {
                return Err(ReadError::Failed);
            }
            let key = u32::from_be_bytes([key[0], key[1], key[2], key[3]]);
            let info = self.call(&SmcKeyData {
                key,
                command: READ_KEY_INFO,
                ..SmcKeyData::default()
            })?;
            let data_size = info.key_info.data_size;
            if data_size == 0 || data_size > 32 {
                return Err(ReadError::Failed);
            }
            let data = self.call(&SmcKeyData {
                key,
                key_info: SmcKeyInfo {
                    data_size,
                    ..SmcKeyInfo::default()
                },
                command: READ_BYTES,
                ..SmcKeyData::default()
            })?;
            Ok((info.key_info.data_type, data.bytes))
        }
    }

    impl Drop for Smc {
        fn drop(&mut self) {
            unsafe { IOServiceClose(self.0) };
        }
    }

    fn number(smc: &Smc, name: &str) -> Result<f32, ReadError> {
        let (kind, bytes) = smc.read(name)?;
        match &kind.to_be_bytes() {
            b"flt " => Ok(f32::from_le_bytes(bytes[..4].try_into().unwrap())),
            b"fpe2" => Ok(u16::from_be_bytes(bytes[..2].try_into().unwrap()) as f32 / 4.0),
            b"ui8 " => Ok(bytes[0] as f32),
            b"ui16" => Ok(u16::from_be_bytes(bytes[..2].try_into().unwrap()) as f32),
            _ => Err(ReadError::Failed),
        }
    }

    fn empty(status: FanStatus) -> FanTelemetry {
        FanTelemetry {
            status,
            fans: Vec::new(),
        }
    }

    pub struct MacSmcFanProvider {
        smc: Result<Smc, ReadError>,
    }

    impl MacSmcFanProvider {
        pub fn new() -> Self {
            Self { smc: Smc::open() }
        }
    }

    impl FanProvider for MacSmcFanProvider {
        fn collect(&mut self) -> FanTelemetry {
            let smc = match &self.smc {
                Ok(smc) => smc,
                Err(ReadError::NotRoot) => return empty(FanStatus::NotRoot),
                Err(_) => return empty(FanStatus::Unavailable),
            };
            let count = match number(smc, "FNum") {
                Ok(count) if count.is_finite() && count >= 0.0 => count as usize,
                Err(ReadError::NotRoot) => return empty(FanStatus::NotRoot),
                Err(ReadError::Missing) => return empty(FanStatus::NoFan),
                _ => return empty(FanStatus::Unavailable),
            };
            if count == 0 {
                return empty(FanStatus::NoFan);
            }

            let mut fans = Vec::new();
            for index in 0..count.min(16) {
                let rpm_key = format!("F{index}Ac");
                match number(smc, &rpm_key) {
                    Ok(rpm) if rpm.is_finite() && rpm >= 0.0 => {
                        let max_key = format!("F{index}Mx");
                        let max_rpm = number(smc, &max_key)
                            .ok()
                            .filter(|value| value.is_finite() && *value > 0.0);
                        fans.push(FanSnapshot {
                            name: format!("Fan {}", index + 1),
                            rpm,
                            max_rpm,
                        });
                    }
                    Err(ReadError::NotRoot) => return empty(FanStatus::NotRoot),
                    _ => {}
                }
            }
            if fans.is_empty() {
                empty(FanStatus::Unavailable)
            } else {
                FanTelemetry {
                    status: FanStatus::Available,
                    fans,
                }
            }
        }
    }
}
