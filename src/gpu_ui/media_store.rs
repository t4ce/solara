use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static STORE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn root() -> PathBuf {
    cache_root().join("media")
}

pub(crate) fn cache_root() -> PathBuf {
    let cache_root = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(".cache"))
        })
        .unwrap_or_else(std::env::temp_dir);
    cache_root.join("solara")
}

pub(crate) fn store_snapshot(
    directory: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    create_store(directory)?;
    let destination = directory.join(file_name);
    if destination.exists() && file_matches(&destination, bytes)? {
        return Ok(destination);
    }

    let temporary = temporary_path(directory, file_name);
    if let Err(error) = write_artifact(&temporary, bytes) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "failed to publish snapshot {}: {error}",
            destination.display()
        ));
    }
    Ok(destination)
}

fn create_store(directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| {
        format!(
            "failed to create media store {}: {error}",
            directory.display()
        )
    })
}

fn temporary_path(directory: &Path, file_name: &str) -> PathBuf {
    let sequence = STORE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    directory.join(format!(
        ".{file_name}.{}.{}.partial",
        std::process::id(),
        sequence
    ))
}

fn write_artifact(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create artifact {}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("failed to store artifact {}: {error}", path.display()))
}

fn file_matches(path: &Path, expected_bytes: &[u8]) -> Result<bool, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("failed to verify artifact {}: {error}", path.display()))?;
    let length_matches = file
        .metadata()
        .map_err(|error| format!("failed to inspect artifact {}: {error}", path.display()))?
        .len()
        == expected_bytes.len() as u64;
    if !length_matches {
        return Ok(false);
    }

    let mut buffer = [0_u8; 16 * 1024];
    for expected in expected_bytes.chunks(buffer.len()) {
        file.read_exact(&mut buffer[..expected.len()])
            .map_err(|error| format!("failed to verify artifact {}: {error}", path.display()))?;
        if &buffer[..expected.len()] != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{STORE_SEQUENCE, store_snapshot};
    use std::sync::atomic::Ordering;

    fn test_directory(label: &str) -> std::path::PathBuf {
        let sequence = STORE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "solara-media-store-{label}-{}-{sequence}",
            std::process::id()
        ))
    }

    #[test]
    fn snapshot_atomically_replaces_older_bytes() {
        let directory = test_directory("snapshot");
        let first = store_snapshot(&directory, "watch.html", b"old").expect("snapshot stores");
        let second = store_snapshot(&directory, "watch.html", b"new").expect("snapshot updates");

        assert_eq!(first, second);
        assert_eq!(std::fs::read(&second).expect("snapshot reads"), b"new");
        std::fs::remove_dir_all(&directory).expect("test media store removes");
    }
}
