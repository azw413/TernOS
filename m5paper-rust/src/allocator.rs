use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;

unsafe extern "C" {
    fn heap_caps_aligned_alloc(alignment: usize, size: usize, caps: u32) -> *mut c_void;
    fn free(ptr: *mut c_void);
}

const MALLOC_CAP_SPIRAM: u32 = 1 << 10;
const MALLOC_CAP_8BIT: u32 = 1 << 2;

struct EspAllocator;

unsafe impl GlobalAlloc for EspAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(core::mem::size_of::<usize>());
        let size = layout.size().max(1);
        let preferred = unsafe {
            heap_caps_aligned_alloc(align, size, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT) as *mut u8
        };
        if !preferred.is_null() {
            return preferred;
        }
        unsafe { heap_caps_aligned_alloc(align, size, MALLOC_CAP_8BIT) as *mut u8 }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { free(ptr as *mut c_void) };
    }
}

#[global_allocator]
static ALLOCATOR: EspAllocator = EspAllocator;

#[alloc_error_handler]
fn alloc_error(layout: Layout) -> ! {
    unsafe {
        crate::ffi::ets_printf(
            b"m5paper-rust: alloc_error size=%u align=%u\n\0".as_ptr(),
            layout.size() as u32,
            layout.align() as u32,
        );
    }
    loop {
        crate::ffi::delay_ms(1000);
    }
}
