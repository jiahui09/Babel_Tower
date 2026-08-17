use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Cursor, Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn publish_bytes(root: &Path, bytes: &[u8]) -> io::Result<([u8; 32], PathBuf)> {
    let (digest, path, _) = publish_reader(root, Cursor::new(bytes))?;
    Ok((digest, path))
}

pub fn publish_reader(root: &Path, mut reader: impl Read) -> io::Result<([u8; 32], PathBuf, u64)> {
    let incoming = root.join("incoming");
    fs::create_dir_all(&incoming)?;
    let temporary_path = incoming.join(format!(".{}.tmp", Uuid::new_v4()));
    let mut temporary = File::create(&temporary_path)?;
    let mut hasher = Sha256::new();
    let mut byte_length = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        temporary.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        byte_length += read as u64;
    }
    temporary.sync_all()?;
    drop(temporary);

    let digest: [u8; 32] = hasher.finalize().into();
    let hex = hex::encode(digest);
    let directory = root.join("sha256").join(&hex[..2]);
    let final_path = directory.join(&hex[2..]);
    if final_path.exists() {
        let existing = sha256_file(&final_path)?;
        if existing != digest {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "CAS object failed hash verification: {}",
                    final_path.display()
                ),
            ));
        }
        fs::remove_file(&temporary_path)?;
        sync_directory(&incoming)?;
        return Ok((digest, final_path, byte_length));
    }

    fs::create_dir_all(&directory)?;
    match fs::hard_link(&temporary_path, &final_path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            verify_object(&final_path, &digest)?;
        }
        Err(error) => return Err(error),
    }
    sync_directory(&directory)?;
    fs::remove_file(&temporary_path)?;
    sync_directory(&incoming)?;
    Ok((digest, final_path, byte_length))
}

pub fn object_path(root: &Path, digest: &[u8; 32]) -> PathBuf {
    let hex = hex::encode(digest);
    root.join("sha256").join(&hex[..2]).join(&hex[2..])
}

pub fn verify_object(path: &Path, expected: &[u8; 32]) -> io::Result<()> {
    let actual = sha256_file(path)?;
    if &actual != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("object hash mismatch: {}", path.display()),
        ));
    }
    Ok(())
}

pub fn cleanup_temporary(root: &Path) -> io::Result<usize> {
    let mut removed = cleanup_temporary_directory(&root.join("incoming"))?;
    let sha_root = root.join("sha256");
    let prefixes = match fs::read_dir(sha_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    for prefix in prefixes {
        let prefix = prefix?;
        if !prefix.file_type()?.is_dir() {
            continue;
        }
        for entry in fs::read_dir(prefix.path())? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.file_type()?.is_file() && name.starts_with('.') && name.ends_with(".tmp") {
                fs::remove_file(entry.path())?;
                removed += 1;
            }
        }
    }
    Ok(removed)
}

fn cleanup_temporary_directory(directory: &Path) -> io::Result<usize> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.file_type()?.is_file() && name.starts_with('.') && name.ends_with(".tmp") {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn sha256_file(path: &Path) -> io::Result<[u8; 32]> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(not(windows))]
pub fn sync_directory(path: &Path) -> io::Result<()> {
    OpenOptions::new().read(true).open(path)?.sync_all()
}

#[cfg(windows)]
pub fn sync_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?
        .sync_all()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn publishing_the_same_content_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let first = publish_bytes(temp.path(), b"immutable source").unwrap();
        let second = publish_bytes(temp.path(), b"immutable source").unwrap();
        assert_eq!(first, second);
        assert_eq!(fs::read(first.1).unwrap(), b"immutable source");
    }

    #[test]
    fn corrupted_existing_object_is_rejected() {
        let temp = TempDir::new().unwrap();
        let (_, path) = publish_bytes(temp.path(), b"immutable source").unwrap();
        fs::write(&path, b"corrupted").unwrap();
        let error = publish_bytes(temp.path(), b"immutable source").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn startup_cleanup_removes_only_interrupted_temporary_objects() {
        let temp = TempDir::new().unwrap();
        let (_, object) = publish_bytes(temp.path(), b"kept").unwrap();
        let temporary = object.parent().unwrap().join(".interrupted.tmp");
        fs::write(&temporary, b"partial").unwrap();
        assert_eq!(cleanup_temporary(temp.path()).unwrap(), 1);
        assert!(object.exists());
        assert!(!temporary.exists());
    }

    #[test]
    fn reader_publication_streams_and_reports_length() {
        let temp = TempDir::new().unwrap();
        let bytes = vec![0x5a; 3 * 64 * 1024 + 17];
        let (hash, path, byte_length) = publish_reader(temp.path(), Cursor::new(&bytes)).unwrap();
        assert_eq!(byte_length, bytes.len() as u64);
        assert_eq!(fs::read(path).unwrap(), bytes);
        let expected: [u8; 32] = Sha256::digest(&bytes).into();
        assert_eq!(hash, expected);
    }
}
