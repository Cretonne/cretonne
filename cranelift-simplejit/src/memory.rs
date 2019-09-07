#[cfg(not(feature = "selinux-fix"))]
use errno;

#[cfg(not(any(feature = "selinux-fix", windows)))]
use libc;

#[cfg(feature = "selinux-fix")]
use memmap::MmapMut;

use region;
use std::mem;
use std::ptr;

/// Round `size` up to the nearest multiple of `page_size`.
fn round_up_to_page_size(size: usize, page_size: usize) -> usize {
    (size + (page_size - 1)) & !(page_size - 1)
}

/// A simple struct consisting of a pointer and length.
struct PtrLen {
    #[cfg(feature = "selinux-fix")]
    map: Option<MmapMut>,

    ptr: *mut u8,
    len: usize,
}

impl PtrLen {
    /// Create a new empty `PtrLen`.
    fn new() -> Self {
        Self {
            #[cfg(feature = "selinux-fix")]
            map: None,

            ptr: ptr::null_mut(),
            len: 0,
        }
    }

    /// Create a new `PtrLen` pointing to at least `size` bytes of memory,
    /// suitably sized and aligned for memory protection.
    #[cfg(all(not(target_os = "windows"), feature = "selinux-fix"))]
    fn with_size(size: usize) -> Result<Self, String> {
        let page_size = region::page::size();
        let alloc_size = round_up_to_page_size(size, page_size);
        let map = MmapMut::map_anon(alloc_size);

        match map {
            Ok(mut map) => {
                // The order here is important; we assign the pointer first to get
                // around compile time borrow errors.
                Ok(Self {
                    ptr: map.as_mut_ptr(),
                    map: Some(map),
                    len: alloc_size,
                })
            }
            Err(e) => Err(e.to_string()),
        }
    }

    #[cfg(all(not(target_os = "windows"), not(feature = "selinux-fix")))]
    fn with_size(size: usize) -> Result<Self, String> {
        let mut ptr = ptr::null_mut();
        let page_size = region::page::size();
        let alloc_size = round_up_to_page_size(size, page_size);
        unsafe {
            let err = libc::posix_memalign(&mut ptr, page_size, alloc_size);

            if err == 0 {
                Ok(Self {
                    ptr: ptr as *mut u8,
                    len: alloc_size,
                })
            } else {
                Err(errno::Errno(err).to_string())
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn with_size(size: usize) -> Result<Self, String> {
        use winapi::um::memoryapi::VirtualAlloc;
        use winapi::um::winnt::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE};

        let page_size = region::page::size();

        // VirtualAlloc always rounds up to the next multiple of the page size
        let ptr = unsafe {
            VirtualAlloc(
                ptr::null_mut(),
                size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        if !ptr.is_null() {
            Ok(Self {
                ptr: ptr as *mut u8,
                len: round_up_to_page_size(size, page_size),
            })
        } else {
            Err(errno::errno().to_string())
        }
    }
}

/// JIT memory manager. This manages pages of suitably aligned and
/// accessible memory.
pub struct Memory {
    allocations: Vec<PtrLen>,
    executable: usize,
    current: PtrLen,
    position: usize,
}

impl Memory {
    pub fn new() -> Self {
        Self {
            allocations: Vec::new(),
            executable: 0,
            current: PtrLen::new(),
            position: 0,
        }
    }

    fn finish_current(&mut self) {
        self.allocations
            .push(mem::replace(&mut self.current, PtrLen::new()));
        self.position = 0;
    }

    /// TODO: Use a proper error type.
    pub fn allocate(&mut self, size: usize, align: u8) -> Result<*mut u8, String> {
        if self.position % align as usize != 0 {
            self.position += align as usize - self.position % align as usize;
            debug_assert!(self.position % align as usize == 0);
        }

        if size <= self.current.len - self.position {
            // TODO: Ensure overflow is not possible.
            let ptr = unsafe { self.current.ptr.add(self.position) };
            self.position += size;
            return Ok(ptr);
        }

        self.finish_current();

        // TODO: Allocate more at a time.
        self.current = PtrLen::with_size(size)?;
        self.position = size;
        Ok(self.current.ptr)
    }

    /// Set all memory allocated in this `Memory` up to now as readable and executable.
    pub fn set_readable_and_executable(&mut self) {
        self.finish_current();

        #[cfg(feature = "selinux-fix")]
        {
            for &PtrLen { ref map, ptr, len } in &self.allocations[self.executable..] {
                if len != 0 && map.is_some() {
                    unsafe {
                        region::protect(ptr, len, region::Protection::ReadExecute)
                            .expect("unable to make memory readable+executable");
                    }
                }
            }
        }

        #[cfg(not(feature = "selinux-fix"))]
        {
            for &PtrLen { ptr, len } in &self.allocations[self.executable..] {
                if len != 0 {
                    unsafe {
                        region::protect(ptr, len, region::Protection::ReadExecute)
                            .expect("unable to make memory readable+executable");
                    }
                }
            }
        }
    }

    /// Set all memory allocated in this `Memory` up to now as readonly.
    pub fn set_readonly(&mut self) {
        self.finish_current();

        #[cfg(feature = "selinux-fix")]
        {
            for &PtrLen { ref map, ptr, len } in &self.allocations[self.executable..] {
                if len != 0 && map.is_some() {
                    unsafe {
                        region::protect(ptr, len, region::Protection::Read)
                            .expect("unable to make memory readonly");
                    }
                }
            }
        }

        #[cfg(not(feature = "selinux-fix"))]
        {
            for &PtrLen { ptr, len } in &self.allocations[self.executable..] {
                if len != 0 {
                    unsafe {
                        region::protect(ptr, len, region::Protection::Read)
                            .expect("unable to make memory readonly");
                    }
                }
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl Drop for PtrLen {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                region::protect(self.ptr, self.len, region::Protection::ReadWrite)
                    .expect("unable to unporotect memory");
                libc::free(self.ptr as _);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_up_to_page_size() {
        assert_eq!(round_up_to_page_size(0, 4096), 0);
        assert_eq!(round_up_to_page_size(1, 4096), 4096);
        assert_eq!(round_up_to_page_size(4096, 4096), 4096);
        assert_eq!(round_up_to_page_size(4097, 4096), 8192);
    }
}
