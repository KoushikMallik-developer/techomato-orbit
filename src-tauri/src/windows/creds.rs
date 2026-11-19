//! Windows Credential Manager as the `SecretStore`. Target: `TechomatoOrbit/<service_id>/<key>`.

use crate::error::{AppError, AppResult, ErrorKind};
use crate::secrets::SecretStore;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_NOT_FOUND, WIN32_ERROR};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredEnumerateW, CredFree, CredReadW, CredWriteW, CREDENTIALW,
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};

/// Windows caps a credential blob at 5 * 512 bytes.
const MAX_BLOB: usize = 2560;

pub struct CredentialManagerStore;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn target(service_id: &str, key: &str) -> Vec<u16> {
    wide(&format!("TechomatoOrbit/{service_id}/{key}"))
}

fn os(what: &str, e: windows::core::Error) -> AppError {
    AppError::new(
        ErrorKind::Os,
        format!("Credential Manager: could not {what}."),
    )
    .with_detail(e.to_string())
}

fn is_not_found(e: &windows::core::Error) -> bool {
    e.code() == ERROR_NOT_FOUND.to_hresult() || e.code() == WIN32_ERROR(1168).to_hresult()
}

impl SecretStore for CredentialManagerStore {
    fn set(&self, service_id: &str, key: &str, value: &str) -> AppResult<()> {
        let blob = value.as_bytes();
        if blob.len() > MAX_BLOB {
            return Err(AppError::new(
                ErrorKind::Validation,
                format!("The secret \"{key}\" is too long to store (limit {MAX_BLOB} bytes)."),
            ));
        }
        let mut name = target(service_id, key);
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(name.as_mut_ptr()),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: `cred` points at buffers that outlive the call; the API copies them.
        unsafe { CredWriteW(&cred, 0) }.map_err(|e| os("save a secret", e))
    }

    fn get(&self, service_id: &str, key: &str) -> AppResult<Option<String>> {
        let name = target(service_id, key);
        let mut out: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: on success `out` is a buffer we free with CredFree after copying.
        let read = unsafe { CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut out) };
        match read {
            Ok(()) => {
                let value = unsafe {
                    let c = &*out;
                    let bytes =
                        std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
                    let v = String::from_utf8_lossy(bytes).into_owned();
                    CredFree(out as *const _);
                    v
                };
                Ok(Some(value))
            }
            Err(e) if is_not_found(&e) => Ok(None),
            Err(e) => Err(os("read a secret", e)),
        }
    }

    fn delete(&self, service_id: &str, key: &str) -> AppResult<()> {
        let name = target(service_id, key);
        // SAFETY: `name` is a valid NUL-terminated wide string.
        match unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) } {
            Ok(()) => Ok(()),
            Err(e) if is_not_found(&e) => Ok(()),
            Err(e) => Err(os("delete a secret", e)),
        }
    }

    fn delete_all(&self, service_id: &str) -> AppResult<()> {
        let filter = wide(&format!("TechomatoOrbit/{service_id}/*"));
        let mut count = 0u32;
        let mut list: *mut *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: on success `list` holds `count` pointers, freed once with CredFree below.
        match unsafe { CredEnumerateW(PCWSTR(filter.as_ptr()), None, &mut count, &mut list) } {
            Ok(()) => {}
            Err(e) if is_not_found(&e) => return Ok(()),
            Err(e) => return Err(os("list secrets", e)),
        }
        let targets: Vec<Vec<u16>> = (0..count as usize)
            .map(|i| unsafe {
                let name = (**list.add(i)).TargetName;
                let mut v = std::slice::from_raw_parts(
                    name.0,
                    (0..).take_while(|&n| *name.0.add(n) != 0).count(),
                )
                .to_vec();
                v.push(0);
                v
            })
            .collect();
        unsafe { CredFree(list as *const _) };
        for t in targets {
            match unsafe { CredDeleteW(PCWSTR(t.as_ptr()), CRED_TYPE_GENERIC, None) } {
                Ok(()) => {}
                Err(e) if is_not_found(&e) => {}
                Err(e) => return Err(os("delete a secret", e)),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::SecretStore;

    /// Uses the real Credential Manager under a unique service id, cleaned up at the end.
    #[test]
    #[ignore]
    fn round_trips_including_non_ascii_and_delete_all_is_scoped() {
        let s = CredentialManagerStore;
        let a = format!("test-{}", std::process::id());
        let b = format!("{a}-other");
        s.set(&a, "TOKEN", "pä55 wörd ✓ 日本語").unwrap();
        s.set(&a, "OTHER", "second").unwrap();
        s.set(&b, "TOKEN", "untouched").unwrap();
        assert_eq!(
            s.get(&a, "TOKEN").unwrap().as_deref(),
            Some("pä55 wörd ✓ 日本語")
        );
        s.set(&a, "TOKEN", "replaced").unwrap();
        assert_eq!(s.get(&a, "TOKEN").unwrap().as_deref(), Some("replaced"));
        s.delete(&a, "OTHER").unwrap();
        assert_eq!(s.get(&a, "OTHER").unwrap(), None);
        s.delete(&a, "OTHER").unwrap(); // deleting a missing secret is not an error
        s.delete_all(&a).unwrap();
        assert_eq!(s.get(&a, "TOKEN").unwrap(), None);
        assert_eq!(s.get(&b, "TOKEN").unwrap().as_deref(), Some("untouched"));
        s.delete_all(&b).unwrap();
    }

    #[test]
    #[ignore]
    fn an_oversized_value_is_refused_with_a_clear_message() {
        let e = CredentialManagerStore
            .set("test-big", "K", &"x".repeat(3000))
            .unwrap_err();
        assert!(e.message.contains("too long"), "{}", e.message);
    }
}
