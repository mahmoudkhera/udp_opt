use anyhow::Result;

pub fn fill_random(buffer: &mut [u8], length: usize) ->Result<()> {
    #[cfg(unix)]
    {
        use std::{fs::File, io::Read};

        use anyhow::Context;
        let _ = length;

        let mut random = File::open("/dev/urandom").context("Failed to open /dev/urandom")?;

        random
            .read_exact(buffer)
            .context("Failed to read random bytes from /dev/urandom")?;

        Ok(())
    }

    #[cfg(windows)]
    {
       // Flags: use the system-preferred RNG without opening an algorithm handle
        const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;

        #[link(name = "bcrypt")]
        unsafe extern "system" {
            fn BCryptGenRandom(
                hAlgorithm: usize,
                pbBuffer: *mut u8,
                cbBuffer: u32,
                dwFlags: u32,
            ) -> i32;
        }

        unsafe {
            let status = BCryptGenRandom(
                0, // use system-preferred RNG
                buffer.as_mut_ptr(),
                length as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            );

            if status != 0 {
                println!("BCryptGenRandom failed with status: {:#x}", status);
            }
        }

     Ok(())
    }
}
