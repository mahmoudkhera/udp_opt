//! # Cross-Platform Random Number Generator
//!
//! Provides sync and async  random number generator for filling buffers with random bytes,
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

pub struct AsyncRandomToSend {
    #[cfg(unix)]
    file: tokio::fs::File,
}

impl AsyncRandomToSend {
    /// Creates a new `AsyncRandomToSend`.
    ///
    /// - On Unix, opens `/dev/urandom` asynchronously.
    /// - On Windows, no actual I/O is required, so this is a fast operation.
    ///
    /// # Errors
    ///
    /// - Returns an `io::Error` if `/dev/urandom` cannot be opened on Unix.
    /// - On Windows, this function always succeeds.
    pub async fn new() -> io::Result<Self> {
        #[cfg(unix)]
        {
            let file = tokio::fs::File::open("/dev/urandom").await?;
            Ok(Self { file })
        }

        #[cfg(windows)]
        {
            Ok(Self {})
        }
    }

    /// Asynchronously fills the given buffer with cryptographically secure random bytes.
    ///
    /// # Parameters
    ///
    /// - `buffer`: A mutable byte slice that will be filled with random data.
    ///
    /// # Errors
    ///
    /// - On Unix, returns any `tokio::io::Error` encountered while reading.
    /// - On Windows, returns an error if `BCryptGenRandom` fails.
    ///

    pub async fn fill(&mut self, buffer: &mut [u8]) -> io::Result<()> {
        #[cfg(unix)]
        {
            use tokio::io::AsyncReadExt;
            let mut total = 0;
            while total < buffer.len() {
                let n = self.file.read(&mut buffer[total..]).await?;
                if n == 0 {
                    break;
                }
                total += n;
            }
            Ok(())
        }

        // note that this functin in non-bloking in  nature
        #[cfg(windows)]
        unsafe {
            let status = BCryptGenRandom(
                0,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            );

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
