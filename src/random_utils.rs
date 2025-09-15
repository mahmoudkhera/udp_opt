use anyhow::Result;

pub fn fill_random(buffer: &mut [u8], length: usize) -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::fs::File;
        use tokio::io::AsyncReadExt;
        let mut random = File::open("/dev/urandom")
            .await
            .context("Failed to open /dev/urandom")?;
        random
            .read_exact(buffer)
            .await
            .context("Failed to read random bytes from /dev/urandom")?;

        Ok(())
    }

    #[cfg(windows)]
    {
        const PROV_RSA_FULL: u32 = 1;
        const CRYPT_VERIFYCONTEXT: u32 = 0xF0000000;

        #[link(name = "advapi32")] //Tells Rust to link against advapi32.dll, which contains the Windows CryptoAPI
        unsafe extern "C" {
            // Acquires a handle to a cryptographic provider.
            fn CryptAcquireContextA(
                hProv: *mut usize,       //pointer to where the handle will be stored
                pszContainer: *const i8, //name of the key to  the container
                pszProvider: *const i8,  //name of crypto provider
                dwProvType: u32,         //type of provider
                dwFlags: u32, //extra options (CRYPT_VERIFYCONTEXT = 0xF0000000 for "temporary, no persisted keys")
            ) -> i32;

            fn CryptGenRandom(
                hProv: usize,      //takes the provider handle
                dwLen: u32,        //number of random bytes to generate
                pbBuffer: *mut u8, //pointer to a buffer that will be filled with random bytes
            ) -> i32;

            //Releases the crypto provider handle when you’re done.
            fn CryptReleaseContext(hProv: usize, dwFlags: u32) -> i32;

        }

        //implementation

        unsafe {
            use std::ptr;
            let mut h_provider = 0;

            // aquire the cryptographic provider context
            let result = CryptAcquireContextA(
                &mut h_provider,
                ptr::null(),
                ptr::null(),
                PROV_RSA_FULL,
                CRYPT_VERIFYCONTEXT,
            );

            if result == 0 {
                panic!("CryptAcquireContextA failed");
            }

            //generate random bytes

            if CryptGenRandom(h_provider, length as u32, buffer.as_mut_ptr()) == 0 {
                CryptReleaseContext(h_provider, 0);
                panic!("CryptGenRandom failed");
            }
        }

        Ok(())
    }
}
