//! # Cross-Platform Random Number Generator
//!
//! Provides a random number generator for filling buffers with random bytes,
//! compatible with both Unix-like systems and Windows.  
//! On Unix, it uses `/dev/urandom`.  
//! On Windows, it uses the system-preferred RNG via `BCryptGenRandom`.

use std::io;

#[cfg(windows)]
/// Flag to use the system-preferred RNG on Windows without opening an algorithm handle
const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;
#[cfg(windows)]
#[link(name = "bcrypt")]

unsafe extern "system" {
    /// External Windows API function for random bytes
    fn BCryptGenRandom(hAlgorithm: usize, pbBuffer: *mut u8, cbBuffer: u32, dwFlags: u32) -> i32;
}

/// Cross-platform random number generator
pub struct RandomToSend {
    /// File handle for Unix systems (`/dev/urandom`)
    #[cfg(unix)]
    file: std::fs::File,
}

impl RandomToSend {
    /// Creates a new random number generator instance
    ///
    /// # Errors
    /// - Unix: if opening `/dev/urandom` fails
    /// - Windows: never fails on creation  
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
    /// Fills the provided buffer with random bytes
    ///
    /// # Parameters
    /// - `buffer`: the mutable slice to fill with random data
    ///
    /// # Errors
    /// - Unix: if reading from `/dev/urandom` fails
    /// - Windows: if `BCryptGenRandom` fails

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
