use std::io;

#[cfg(windows)]

// Flags: use the system-preferred RNG without opening an algorithm handle
const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;
#[cfg(windows)]
#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(hAlgorithm: usize, pbBuffer: *mut u8, cbBuffer: u32, dwFlags: u32) -> i32;
}

pub struct RandomToSend {
    #[cfg(unix)]
    file: std::fs::File,
}

impl RandomToSend {
    pub fn new() -> io::Result<Self> {
        #[cfg(unix)]
        {
            let file = std::fs::File::open("/dev/urandom")?;
            Ok(Self { file })
        }

        #[cfg(windows)]
        {
            Ok(Self {})
        }
    }

    pub fn fill(&mut self, buffer: &mut [u8]) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::io::Read;

            let mut total_read = 0;
            while total_read < buffer.len() {
                match self.file.read(&mut buffer[total_read..])? {
                    0 => break, // EOF
                    n => total_read += n,
                }
            }
            Ok(())
        }

        #[cfg(windows)]
        {
            let status = unsafe {
                BCryptGenRandom(
                    0,
                    buffer.as_mut_ptr(),
                    buffer.len() as u32,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                )
            };

            if status != 0 {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("BCryptGenRandom failed {:#x}", status),
                ))
            } else {
                Ok(())
            }
        }
    }
}
