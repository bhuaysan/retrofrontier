use crate::domain::runtime::{RuntimeError, Sha256Digest};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub const HASH_BUFFER_BYTES: usize = 128 * 1024;

pub fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    let digest = Sha256::digest(bytes);
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    Sha256Digest::from_bytes(output)
}

pub fn sha256_file(path: &Path) -> Result<(u64, Sha256Digest), RuntimeError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    let mut size = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| RuntimeError::Integrity("file size overflow".to_owned()))?;
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    Ok((size, Sha256Digest::from_bytes(output)))
}

pub fn verify_file(
    path: &Path,
    expected_size: u64,
    expected_hash: Sha256Digest,
) -> Result<(), RuntimeError> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(RuntimeError::Integrity(format!(
            "download target is not a regular file: {}",
            path.display()
        )));
    }
    if metadata.len() != expected_size {
        return Err(RuntimeError::Integrity(format!(
            "{} has size {}, expected {expected_size}",
            path.display(),
            metadata.len()
        )));
    }
    let (actual_size, actual_hash) = sha256_file(path)?;
    if actual_size != expected_size || actual_hash != expected_hash {
        return Err(RuntimeError::Integrity(format!(
            "{} failed SHA-256 verification",
            path.display()
        )));
    }
    Ok(())
}

pub fn copy_with_limit<R: Read + ?Sized, W: io::Write>(
    reader: &mut R,
    writer: &mut W,
    max_bytes: u64,
) -> Result<u64, RuntimeError> {
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    let mut total = 0_u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| RuntimeError::Download("download size overflow".to_owned()))?;
        if total > max_bytes {
            return Err(RuntimeError::Download(format!(
                "download exceeded the {max_bytes}-byte limit"
            )));
        }
        writer.write_all(&buffer[..read])?;
    }
    Ok(total)
}
