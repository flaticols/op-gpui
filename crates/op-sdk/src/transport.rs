#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(target_os = "macos")]
use std::sync::Arc;

use crate::Result;

pub(crate) trait Transport: Send + Sync {
    fn call(&self, input: &[u8]) -> Result<Vec<u8>>;
}

#[cfg(target_os = "macos")]
pub(crate) fn load(path: &Path) -> Result<Arc<dyn Transport>> {
    platform::load(path)
}

#[cfg(target_os = "macos")]
mod platform {
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        ptr, slice,
        sync::{Arc, Mutex, OnceLock, Weak},
    };

    use libloading::Library;

    use super::Transport;
    use crate::{Error, Result};

    type SendMessage =
        unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize, *mut usize) -> i32;
    type FreeResponse = unsafe extern "C" fn(*mut u8, usize, usize);

    struct DynamicLibrary {
        _library: Library,
        send_message: SendMessage,
        free_response: FreeResponse,
        call_lock: Mutex<()>,
    }

    impl Transport for DynamicLibrary {
        fn call(&self, input: &[u8]) -> Result<Vec<u8>> {
            if input.is_empty() {
                return Err(Error::Protocol(
                    "refusing to send an empty request".to_owned(),
                ));
            }
            let _guard = self.call_lock.lock().map_err(|_| Error::LockPoisoned)?;
            call_symbols(self.send_message, self.free_response, input)
        }
    }

    fn call_symbols(
        send_message: SendMessage,
        free_response: FreeResponse,
        input: &[u8],
    ) -> Result<Vec<u8>> {
        let mut output = ptr::null_mut();
        let mut output_len = 0usize;
        let mut output_capacity = 0usize;
        // SAFETY: both symbols were loaded from the retained Library with the official ABI.
        let code = unsafe {
            send_message(
                input.as_ptr(),
                input.len(),
                &mut output,
                &mut output_len,
                &mut output_capacity,
            )
        };

        let response = if output.is_null() {
            Vec::new()
        } else if output_len > isize::MAX as usize {
            // SAFETY: the dylib created this allocation and supplied its length/capacity.
            unsafe { free_response(output, output_len, output_capacity) };
            return Err(Error::Protocol(
                "desktop response exceeds addressable memory".to_owned(),
            ));
        } else {
            // SAFETY: successful ABI calls return a readable buffer of output_len bytes.
            let copied = unsafe { slice::from_raw_parts(output, output_len) }.to_vec();
            // SAFETY: the buffer must be returned to the allocator that created it.
            unsafe { free_response(output, output_len, output_capacity) };
            copied
        };

        match code {
            0 => Ok(response),
            -3 => Err(Error::ChannelClosed),
            -7 => Err(Error::ConnectionDropped),
            other => Err(Error::TransportCode(other)),
        }
    }

    type Registry = Mutex<HashMap<PathBuf, Weak<DynamicLibrary>>>;
    static LIBRARIES: OnceLock<Registry> = OnceLock::new();

    pub(super) fn load(path: &Path) -> Result<Arc<dyn Transport>> {
        let path = path.to_path_buf();
        let registry = LIBRARIES.get_or_init(|| Mutex::new(HashMap::new()));
        let mut libraries = registry.lock().map_err(|_| Error::LockPoisoned)?;
        if let Some(library) = libraries.get(&path).and_then(Weak::upgrade) {
            return Ok(library);
        }

        // SAFETY: symbol use is constrained to the declared C ABI and Library stays retained.
        let library = unsafe { Library::new(&path) }.map_err(|error| Error::LibraryLoad {
            path: path.clone(),
            message: error.to_string(),
        })?;
        // SAFETY: the installed SDK defines this exact function signature.
        let send_message = unsafe {
            *library
                .get::<SendMessage>(b"op_sdk_ipc_send_message\0")
                .map_err(|error| Error::MissingSymbol {
                    symbol: "op_sdk_ipc_send_message",
                    message: error.to_string(),
                })?
        };
        // SAFETY: the installed SDK defines this exact function signature.
        let free_response = unsafe {
            *library
                .get::<FreeResponse>(b"op_sdk_ipc_free_response\0")
                .map_err(|error| Error::MissingSymbol {
                    symbol: "op_sdk_ipc_free_response",
                    message: error.to_string(),
                })?
        };
        let loaded = Arc::new(DynamicLibrary {
            _library: library,
            send_message,
            free_response,
            call_lock: Mutex::new(()),
        });
        libraries.insert(path, Arc::downgrade(&loaded));
        Ok(loaded)
    }

    #[cfg(test)]
    mod tests {
        use std::{
            mem,
            sync::atomic::{AtomicI32, AtomicUsize, Ordering},
        };

        use super::*;

        static RETURN_CODE: AtomicI32 = AtomicI32::new(0);
        static FREE_COUNT: AtomicUsize = AtomicUsize::new(0);

        unsafe extern "C" fn fake_send(
            _: *const u8,
            _: usize,
            output: *mut *mut u8,
            output_len: *mut usize,
            output_capacity: *mut usize,
        ) -> i32 {
            let mut response = b"fixture response".to_vec();
            // SAFETY: the test caller supplies writable out-parameter pointers.
            unsafe {
                *output = response.as_mut_ptr();
                *output_len = response.len();
                *output_capacity = response.capacity();
            }
            mem::forget(response);
            RETURN_CODE.load(Ordering::SeqCst)
        }

        unsafe extern "C" fn fake_free(buffer: *mut u8, len: usize, capacity: usize) {
            FREE_COUNT.fetch_add(1, Ordering::SeqCst);
            // SAFETY: fake_send allocated this exact Vec and transferred ownership.
            drop(unsafe { Vec::from_raw_parts(buffer, len, capacity) });
        }

        #[test]
        fn ffi_response_is_freed_on_success() {
            RETURN_CODE.store(0, Ordering::SeqCst);
            FREE_COUNT.store(0, Ordering::SeqCst);
            let response = call_symbols(fake_send, fake_free, b"request").unwrap();
            assert_eq!(response, b"fixture response");
            assert_eq!(FREE_COUNT.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn ffi_response_is_freed_on_error() {
            RETURN_CODE.store(-3, Ordering::SeqCst);
            FREE_COUNT.store(0, Ordering::SeqCst);
            assert!(matches!(
                call_symbols(fake_send, fake_free, b"request"),
                Err(Error::ChannelClosed)
            ));
            assert_eq!(FREE_COUNT.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn installed_library_exports_expected_symbols_when_present() {
            let path = Path::new("/Applications").join(crate::DYLIB_RELATIVE_PATH);
            if path.is_file() {
                load(&path).expect("installed 1Password dylib should expose the SDK ABI");
            }
        }
    }
}
