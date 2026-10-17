//! Protect secrets at rest.
//!
//! On Windows the whole identity/device file is wrapped with DPAPI
//! (user-scoped), so a copied file is useless on another account or machine.
//! Other platforms keep the bytes as-is; the desktop target is Windows.

pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    platform::protect(data)
}

pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    platform::unprotect(data)
}

#[cfg(windows)]
mod platform {
    use std::ptr;

    use windows_sys::Win32::Foundation::{HLOCAL, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(data);
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(format!(
                "DPAPI 加密失败: {}",
                std::io::Error::last_os_error()
            ));
        }
        let result = copy_and_free(&output);
        if result.is_empty() {
            Err("DPAPI 加密返回空数据".into())
        } else {
            Ok(result)
        }
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(data);
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(format!(
                "DPAPI 解密失败: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(copy_and_free(&output))
    }

    fn copy_and_free(blob: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if blob.pbData.is_null() || blob.cbData == 0 {
            return Vec::new();
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec() };
        unsafe {
            LocalFree(blob.pbData as HLOCAL);
        }
        bytes
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
        tracing::warn!("DPAPI is unavailable on this platform; secrets are stored unencrypted");
        Ok(data.to_vec())
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
        Ok(data.to_vec())
    }
}
