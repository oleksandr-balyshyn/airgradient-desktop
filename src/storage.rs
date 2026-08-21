//! Durable file writing shared by the config and history files.
//!
//! Both files are rewritten in place while the app is running -- settings on
//! every save, history on every refresh -- so both are exposed to being
//! interrupted halfway through a write.

use std::env;
use std::fs::{create_dir_all, rename, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Extension used for the temporary file a write goes to first.
const PENDING_EXTENSION: &str = "tmp";

/// Directory this app owns inside whichever XDG base directory is in use.
const APP_DIRECTORY: &str = "airgradient-desktop";

/// The app's directory under one of the XDG base directories.
///
/// The XDG Base Directory specification puts settings under `$XDG_CONFIG_HOME`
/// and recorded data under `$XDG_DATA_HOME`, which is why this app keeps its
/// config file and its history file apart. Both lookups follow the same three
/// steps, so they are written once here: use the base variable if it is set,
/// otherwise the conventional path under `$HOME`, otherwise the current
/// directory, so the path is always deterministic even in an environment that
/// sets neither.
///
/// `base_variable` is the environment variable to try first, for example
/// `XDG_DATA_HOME`; `home_fallback` is the path under `$HOME` the specification
/// names as that variable's default, for example `.local/share`.
pub fn xdg_app_dir(base_variable: &str, home_fallback: &str) -> PathBuf {
    let is_set = |value: &String| !value.trim().is_empty();

    env::var(base_variable)
        .ok()
        .filter(is_set)
        .map(PathBuf::from)
        .or_else(|| {
            env::var("HOME")
                .ok()
                .filter(is_set)
                .map(|home| PathBuf::from(home).join(home_fallback))
        })
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIRECTORY)
}

/// Write a file so that it is either fully replaced or left untouched.
///
/// The obvious implementation, `fs::write`, truncates the file and then writes
/// it. Between those two steps the file on disk is short or empty, so an app
/// that is killed, a machine that loses power, or a disk that fills up at that
/// moment leaves a corrupt file behind. For the config file that means losing
/// the user's settings; for the history file it means losing every recorded
/// reading.
///
/// Writing to a temporary file in the same directory and renaming it over the
/// target avoids that: a rename within one filesystem is atomic, so a reader
/// sees either the previous contents or the new ones, never a half-written mix.
/// The temporary file has to be a sibling because a rename across filesystems is
/// not atomic and, in a sandboxed package, may not be permitted at all.
pub fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    let directory = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no parent directory to write into", path.display()),
        )
    })?;
    create_dir_all(directory)?;

    let pending = path.with_extension(PENDING_EXTENSION);

    // Scoped so the file is flushed and closed before the rename. `sync_all`
    // asks the filesystem to put the contents on disk first, so a crash cannot
    // leave the rename visible while the data behind it is not.
    {
        let mut file = File::create(&pending)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }

    rename(&pending, path)
}

#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::fs::{create_dir_all, read_to_string, remove_dir_all, write};
    use std::path::PathBuf;

    use super::write_atomically;

    fn scratch(name: &str) -> PathBuf {
        let directory = temp_dir().join(format!("airgradient-storage-{name}"));
        let _ = remove_dir_all(&directory);
        create_dir_all(&directory).expect("scratch directory should be creatable");
        directory
    }

    #[test]
    fn writes_contents_to_a_new_file() {
        let path = scratch("new").join("out.json");

        write_atomically(&path, "{\"a\":1}").expect("write should succeed");

        assert_eq!(
            read_to_string(&path).expect("file should exist"),
            "{\"a\":1}"
        );
    }

    #[test]
    fn replaces_the_previous_contents_completely() {
        let path = scratch("replace").join("out.json");
        write(&path, "a much longer set of previous contents").expect("seed write");

        write_atomically(&path, "short").expect("write should succeed");

        assert_eq!(read_to_string(&path).expect("file should exist"), "short");
    }

    #[test]
    fn creates_missing_parent_directories() {
        let path = scratch("nested")
            .join("deep")
            .join("deeper")
            .join("out.json");

        write_atomically(&path, "value").expect("write should create the directories");

        assert_eq!(read_to_string(&path).expect("file should exist"), "value");
    }

    #[test]
    fn leaves_no_temporary_file_behind() {
        let directory = scratch("cleanup");
        let path = directory.join("out.json");

        write_atomically(&path, "value").expect("write should succeed");

        let leftovers: Vec<_> = directory
            .read_dir()
            .expect("directory should be readable")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter(|name| name != "out.json")
            .collect();
        assert!(leftovers.is_empty(), "unexpected leftovers: {leftovers:?}");
    }
}
