use crate::errno;
use std::fs::File;
use std::io::Read;

const MAX_GETENTROPY_LEN: usize = 256;

pub fn getrandom(buffer: &mut [u8], flags: u32) -> Result<usize, i32> {
    if buffer.is_empty() {
        return Ok(0);
    }

    let mut written = 0;
    while written < buffer.len() {
        // SAFETY: This is the narrow kernel-entropy FFI boundary. The slice is
        // valid for `buffer.len() - written` bytes and libc owns errno updates.
        let rc = unsafe {
            libc::getrandom(
                buffer[written..].as_mut_ptr().cast(),
                buffer.len() - written,
                flags,
            )
        };
        if rc == -1 {
            let code = errno::capture_last_os_error();
            if code == libc::EINTR {
                continue;
            }
            if code == libc::ENOSYS {
                fill_from_urandom(&mut buffer[written..])?;
                return Ok(buffer.len());
            }
            return Err(code);
        }
        if rc == 0 {
            errno::set(libc::EIO);
            return Err(libc::EIO);
        }
        written += rc as usize;
    }
    Ok(written)
}

pub fn getentropy(buffer: &mut [u8]) -> Result<(), i32> {
    if buffer.len() > MAX_GETENTROPY_LEN {
        errno::set(libc::EIO);
        return Err(libc::EIO);
    }
    getrandom(buffer, 0).map(|_| ())
}

pub fn arc4random_buf(buffer: &mut [u8]) {
    if getrandom(buffer, 0).is_err() {
        fatal_entropy_failure();
    }
}

pub fn arc4random() -> u32 {
    let mut word = [0u8; 4];
    arc4random_buf(&mut word);
    u32::from_ne_bytes(word)
}

pub fn arc4random_uniform(upper_bound: u32) -> u32 {
    if upper_bound <= 1 {
        return 0;
    }
    let zone = u32::MAX - (u32::MAX % upper_bound);
    loop {
        let value = arc4random();
        if value < zone {
            return value % upper_bound;
        }
    }
}

pub fn random_u64() -> Result<u64, i32> {
    let mut word = [0u8; 8];
    getrandom(&mut word, 0)?;
    Ok(u64::from_ne_bytes(word))
}

fn fill_from_urandom(buffer: &mut [u8]) -> Result<(), i32> {
    let mut file = File::open("/dev/urandom").map_err(|error| capture_io_error(&error))?;
    file.read_exact(buffer)
        .map_err(|error| capture_io_error(&error))?;
    Ok(())
}

fn capture_io_error(error: &std::io::Error) -> i32 {
    let code = error.raw_os_error().unwrap_or(libc::EIO);
    errno::set(code);
    code
}

fn fatal_entropy_failure() -> ! {
    eprintln!("Fatal glibc error: cannot get entropy for arc4random");
    std::process::abort();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getentropy_rejects_large_buffers() {
        let mut buffer = [0u8; 257];
        assert_eq!(getentropy(&mut buffer), Err(libc::EIO));
        assert_eq!(errno::get(), libc::EIO);
    }

    #[test]
    fn arc4random_uniform_stays_bounded() {
        for _ in 0..128 {
            assert!(arc4random_uniform(17) < 17);
        }
    }
}
