//! Native-only device identity storage. Callers supply the trusted app-data
//! directory, never a path received from a renderer or network request.
//! DPAPI protects at rest as the current Windows user, not against code already
//! executing as that user. Returned plaintext must not enter logs or evidence.
#![allow(dead_code)] // Pairing will consume this seam; it creates no keys itself.

use std::path::Path;

const SLOT: &str = "device-identity.dpapi";
const PLAIN_LIMIT: usize = 64 * 1024;
const SEALED_LIMIT: usize = 128 * 1024;
const PURPOSE: &[u8] = b"bob-device-identity-v1";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum VaultError {
    Unsupported,
    InvalidInput,
    UnsafePath,
    AlreadyExists,
    Unavailable,
    ProtectionFailed,
    InvalidProtectedData,
}

/// Creates one protected identity record, never overwriting an existing one.
/// `root` must already exist and be the native host's private app-data directory.
/// A failed/partial write is retained and blocks replacement; do not silently
/// replace identity after a storage failure.
pub(crate) fn store_new(root: &Path, plaintext: &[u8]) -> Result<(), VaultError> {
    #[cfg(windows)]
    {
        windows::store_new(root, plaintext)
    }
    #[cfg(not(windows))]
    {
        let _ = (root, plaintext);
        Err(VaultError::Unsupported)
    }
}

pub(crate) fn read(root: &Path) -> Result<Vec<u8>, VaultError> {
    #[cfg(windows)]
    {
        windows::read(root)
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        Err(VaultError::Unsupported)
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        ffi::c_void,
        fs::{File, OpenOptions},
        io::{Read, Write},
        os::windows::fs::{MetadataExt, OpenOptionsExt},
        ptr,
    };

    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const REPARSE_POINT: u32 = 0x400;
    const UI_FORBIDDEN: u32 = 1;

    #[repr(C)]
    struct Blob {
        len: u32,
        data: *mut u8,
    }

    #[link(name = "Crypt32")]
    extern "system" {
        fn CryptProtectData(
            data: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            data: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "Kernel32")]
    extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    struct OwnedBlob(Blob);
    impl Drop for OwnedBlob {
        fn drop(&mut self) {
            if !self.0.data.is_null() {
                // Do not leave the native plaintext allocation behind after read.
                unsafe {
                    for offset in 0..self.0.len as usize {
                        ptr::write_volatile(self.0.data.add(offset), 0);
                    }
                    LocalFree(self.0.data.cast());
                }
            }
        }
    }

    fn transform(input: &[u8], entropy: &[u8], protect: bool) -> Result<Vec<u8>, VaultError> {
        let limit = if protect { PLAIN_LIMIT } else { SEALED_LIMIT };
        if input.is_empty() || input.len() > limit || entropy.len() > PLAIN_LIMIT {
            return Err(VaultError::InvalidInput);
        }
        let data = Blob {
            len: input.len() as u32,
            data: input.as_ptr() as *mut u8,
        };
        let context = Blob {
            len: entropy.len() as u32,
            data: entropy.as_ptr() as *mut u8,
        };
        let mut output = OwnedBlob(Blob {
            len: 0,
            data: ptr::null_mut(),
        });
        let ok = unsafe {
            if protect {
                CryptProtectData(
                    &data,
                    ptr::null(),
                    &context,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    UI_FORBIDDEN,
                    &mut output.0,
                )
            } else {
                CryptUnprotectData(
                    &data,
                    ptr::null_mut(),
                    &context,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    UI_FORBIDDEN,
                    &mut output.0,
                )
            }
        };
        let output_limit = if protect { SEALED_LIMIT } else { PLAIN_LIMIT };
        if ok == 0
            || output.0.data.is_null()
            || output.0.len == 0
            || output.0.len as usize > output_limit
        {
            return Err(if protect {
                VaultError::ProtectionFailed
            } else {
                VaultError::InvalidProtectedData
            });
        }
        Ok(unsafe { std::slice::from_raw_parts(output.0.data, output.0.len as usize) }.to_vec())
    }

    fn checked_root(root: &Path) -> Result<File, VaultError> {
        if !root.is_absolute() {
            return Err(VaultError::UnsafePath);
        }
        // Deny delete sharing, pinning the final directory while this handle is
        // alive. Intermediate ancestors must be trusted app-data components.
        let handle = OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(BACKUP_SEMANTICS | OPEN_REPARSE_POINT)
            .open(root)
            .map_err(|_| VaultError::Unavailable)?;
        let metadata = handle.metadata().map_err(|_| VaultError::Unavailable)?;
        if !metadata.is_dir() || metadata.file_attributes() & REPARSE_POINT != 0 {
            return Err(VaultError::UnsafePath);
        }
        Ok(handle)
    }

    fn checked_file(file: &File) -> Result<(), VaultError> {
        let metadata = file.metadata().map_err(|_| VaultError::Unavailable)?;
        if !metadata.is_file() || metadata.file_attributes() & REPARSE_POINT != 0 {
            return Err(VaultError::UnsafePath);
        }
        Ok(())
    }

    pub(super) fn store_new(root: &Path, plaintext: &[u8]) -> Result<(), VaultError> {
        let _root_guard = checked_root(root)?;
        let sealed = transform(plaintext, PURPOSE, true)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .custom_flags(OPEN_REPARSE_POINT)
            .open(root.join(SLOT))
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    VaultError::AlreadyExists
                } else {
                    VaultError::Unavailable
                }
            })?;
        checked_file(&file)?;
        file.write_all(&sealed)
            .map_err(|_| VaultError::Unavailable)?;
        file.sync_all().map_err(|_| VaultError::Unavailable)
    }

    pub(super) fn read(root: &Path) -> Result<Vec<u8>, VaultError> {
        let _root_guard = checked_root(root)?;
        let file = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .custom_flags(OPEN_REPARSE_POINT | BACKUP_SEMANTICS)
            .open(root.join(SLOT))
            .map_err(|_| VaultError::Unavailable)?;
        checked_file(&file)?;
        let mut sealed = Vec::new();
        file.take((SEALED_LIMIT + 1) as u64)
            .read_to_end(&mut sealed)
            .map_err(|_| VaultError::Unavailable)?;
        transform(&sealed, PURPOSE, false)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            path::PathBuf,
            sync::atomic::{AtomicU64, Ordering},
            time::{SystemTime, UNIX_EPOCH},
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        struct Temp(PathBuf);
        impl Temp {
            fn new() -> Self {
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let path = std::env::temp_dir().join(format!(
                    "bob-vault-canary-{}-{stamp}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir(&path).unwrap();
                Self(path)
            }
        }
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(self.0.join(SLOT));
                let _ = std::fs::remove_dir(&self.0);
            }
        }
        #[test]
        fn current_user_roundtrip_and_create_new() {
            let root = Temp::new();
            let canary = b"fake-device-canary-not-a-credential";
            store_new(&root.0, canary).unwrap();
            assert_eq!(read(&root.0).unwrap(), canary);
            let original = std::fs::read(root.0.join(SLOT)).unwrap();
            assert!(!original
                .windows(canary.len())
                .any(|window| window == canary));
            assert_eq!(
                store_new(&root.0, b"different-fake"),
                Err(VaultError::AlreadyExists)
            );
            assert_eq!(std::fs::read(root.0.join(SLOT)).unwrap(), original);
        }
        #[test]
        fn wrong_entropy_and_tampering_fail_closed() {
            let sealed = transform(b"fake-canary", PURPOSE, true).unwrap();
            assert_eq!(
                transform(&sealed, b"different-purpose", false),
                Err(VaultError::InvalidProtectedData)
            );
            let mut changed = sealed;
            let last = changed.len() - 1;
            changed[last] ^= 0x80;
            assert_eq!(
                transform(&changed, PURPOSE, false),
                Err(VaultError::InvalidProtectedData)
            );
            let root = Temp::new();
            std::fs::write(root.0.join(SLOT), changed).unwrap();
            assert_eq!(read(&root.0), Err(VaultError::InvalidProtectedData));
        }
        #[test]
        fn bounds_and_unsafe_root_do_not_write() {
            let root = Temp::new();
            assert_eq!(store_new(&root.0, &[]), Err(VaultError::InvalidInput));
            assert_eq!(
                store_new(&root.0, &vec![1; PLAIN_LIMIT + 1]),
                Err(VaultError::InvalidInput)
            );
            assert!(!root.0.join(SLOT).exists());
            assert_eq!(
                store_new(Path::new("relative"), b"fake"),
                Err(VaultError::UnsafePath)
            );
            std::fs::create_dir(root.0.join(SLOT)).unwrap();
            assert!(read(&root.0).is_err());
            std::fs::remove_dir(root.0.join(SLOT)).unwrap();
        }

        #[test]
        fn junction_root_and_slot_are_rejected() {
            let root = Temp::new();
            let target = Temp::new();
            let link = root.0.join(SLOT);
            // NTFS junctions need no symlink privilege. Only generated temporary
            // canary paths are supplied; no existing user path is touched.
            let result = std::process::Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&link)
                .arg(&target.0)
                .output()
                .unwrap();
            assert!(result.status.success(), "canary junction creation failed");
            let root_result = store_new(&link, b"fake-canary");
            let read_result = read(&root.0);
            let store_result = store_new(&root.0, b"fake-canary");
            // Remove only the junction, never recurse through its target.
            std::fs::remove_dir(&link).unwrap();
            assert_eq!(root_result, Err(VaultError::UnsafePath));
            assert_eq!(read_result, Err(VaultError::UnsafePath));
            assert!(store_result.is_err());
            assert!(!target.0.join(SLOT).exists());
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    #[test]
    fn unsupported_platform_never_falls_back_to_plaintext() {
        assert_eq!(
            store_new(Path::new("unused"), b"fake"),
            Err(VaultError::Unsupported)
        );
        assert_eq!(read(Path::new("unused")), Err(VaultError::Unsupported));
    }
}
