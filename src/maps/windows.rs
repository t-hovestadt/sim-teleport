use super::MapError;
use std::ffi::c_void;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    Security::{
        InitializeSecurityDescriptor, SetSecurityDescriptorDacl, SECURITY_ATTRIBUTES,
        SECURITY_DESCRIPTOR,
    },
    System::Memory::{
        CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
        FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
    },
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct WindowsSharedMap {
    h_map: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    size: usize,
}

// Safety: WindowsSharedMap wraps raw Win32 handles (HANDLE, MEMORY_MAPPED_VIEW_ADDRESS).
// The handles are not reference-counted by Windows — each instance owns its own mapping.
// Sending to another thread is safe provided only one thread accesses the view at a time,
// which the caller guarantees. Sync (shared &self access) is safe because as_slice() returns
// a read-only view and no method mutates through a shared reference.
unsafe impl Send for WindowsSharedMap {}
unsafe impl Sync for WindowsSharedMap {}

impl Drop for WindowsSharedMap {
    fn drop(&mut self) {
        unsafe {
            if !self.view.Value.is_null() {
                UnmapViewOfFile(self.view);
                self.view.Value = std::ptr::null_mut();
            }
            if self.h_map != 0 && self.h_map != INVALID_HANDLE_VALUE {
                CloseHandle(self.h_map);
                self.h_map = 0;
            }
        }
    }
}

impl WindowsSharedMap {
    /// Open an existing named shared memory region (source side — read-only).
    /// Uses VirtualQuery to determine the actual mapped size.
    pub fn open(name: &str) -> Result<Self, MapError> {
        unsafe {
            let h_map = OpenFileMappingW(FILE_MAP_READ, 0, wide(name).as_ptr());
            if h_map == 0 || h_map == INVALID_HANDLE_VALUE {
                return Err(MapError::Unavailable);
            }

            let view = MapViewOfFile(h_map, FILE_MAP_READ, 0, 0, 0);
            if view.Value.is_null() {
                CloseHandle(h_map);
                return Err(MapError::Unavailable);
            }

            let size = query_region_size(view.Value as *const u8).unwrap_or(0);
            if size == 0 {
                UnmapViewOfFile(view);
                CloseHandle(h_map);
                return Err(MapError::Unavailable);
            }

            Ok(Self { h_map, view, size })
        }
    }

    /// Create a new pagefile-backed named shared memory region (target side — writable).
    /// Uses an explicit NULL DACL so any process can open the map by name, regardless
    /// of user account or elevation level. Matches iRacing's own shared memory setup.
    pub fn create(name: &str, size: usize) -> Result<Self, MapError> {
        unsafe {
            // Build an explicit NULL DACL security descriptor.
            // A null lpSecurityDescriptor would use the default SD (inherits from creator),
            // which may block access from differently-elevated processes. A NULL DACL grants
            // all access to all principals — identical to what iRacing sets on its own maps.
            let mut sd = std::mem::zeroed::<SECURITY_DESCRIPTOR>();
            InitializeSecurityDescriptor(
                &mut sd as *mut _ as *mut c_void,
                1, // SECURITY_DESCRIPTOR_REVISION
            );
            SetSecurityDescriptorDacl(
                &mut sd as *mut _ as *mut c_void,
                1,                    // bDaclPresent = TRUE
                std::ptr::null_mut(), // NULL DACL — grants all access
                0,                    // bDaclDefaulted = FALSE
            );

            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: &mut sd as *mut _ as *mut c_void,
                bInheritHandle: 0,
            };

            let h_map = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                &sa,
                PAGE_READWRITE,
                (size >> 32) as u32,
                (size & 0xFFFF_FFFF) as u32,
                wide(name).as_ptr(),
            );
            if h_map == 0 {
                return Err(MapError::Other(format!(
                    "CreateFileMappingW failed for {name}"
                )));
            }

            let view = MapViewOfFile(h_map, FILE_MAP_ALL_ACCESS, 0, 0, 0);
            if view.Value.is_null() {
                CloseHandle(h_map);
                return Err(MapError::Other(format!("MapViewOfFile failed for {name}")));
            }

            Ok(Self { h_map, view, size })
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.view.Value as *const u8, self.size) }
    }

    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.view.Value as *mut u8, self.size) }
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

/// Zero a named shared memory map via a temporary write handle.
///
/// FanaLab workaround: zero shared memory on game exit so FanaLab reads RPM=0
/// and sends LED-off to the wheel base firmware. Without this, FanaLab reads
/// stale RPM data and LEDs stay lit indefinitely.
/// See: https://forum.fanatec.com/topic/19449
///
/// Call for each map name (physics, graphics, static) BEFORE dropping the
/// read handles — our handles keep the map alive for FanaLab to read the
/// zeroed data before we release them.
pub fn zero_named_map(name: &str) {
    unsafe {
        let h_map = OpenFileMappingW(FILE_MAP_WRITE, 0, wide(name).as_ptr());
        if h_map == 0 || h_map == INVALID_HANDLE_VALUE {
            return;
        }
        let view = MapViewOfFile(h_map, FILE_MAP_WRITE, 0, 0, 0);
        if view.Value.is_null() {
            CloseHandle(h_map);
            return;
        }
        let size = query_region_size(view.Value as *const u8).unwrap_or(0);
        if size > 0 {
            std::ptr::write_bytes(view.Value as *mut u8, 0, size);
        }
        UnmapViewOfFile(view);
        CloseHandle(h_map);
    }
}

fn query_region_size(ptr: *const u8) -> Option<usize> {
    use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION};
    unsafe {
        let mut mbi = std::mem::zeroed::<MEMORY_BASIC_INFORMATION>();
        let ret = VirtualQuery(
            ptr as *const c_void,
            &mut mbi,
            std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        );
        if ret == 0 {
            None
        } else {
            Some(mbi.RegionSize)
        }
    }
}
