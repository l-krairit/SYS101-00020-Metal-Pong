#[global_allocator]
static ALLOCATOR: DummyAllocator = DummyAllocator;

use alloc::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use core::fmt::Write;

use crate::serial;
pub struct DummyAllocator;

pub static mut HEAP_START: usize = 0x0;
pub static mut OFFSET: usize = 0x0;
pub const HEAP_SIZE: usize = 100 * 1024; // 100 KiB

unsafe impl GlobalAlloc for DummyAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();

        // Align the allocation start
        let alloc_start = (HEAP_START + OFFSET + align - 1) & !(align - 1);
        let alloc_end = alloc_start + size;

        // Check if we exceed heap size
        if alloc_end > HEAP_START + HEAP_SIZE {
            return null_mut();
        }

        // Update OFFSET and return pointer
        OFFSET = alloc_end - HEAP_START;
        alloc_start as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        writeln!(serial(), "dealloc was called at {_ptr:?}").unwrap();
    }
}

pub fn init_heap(offset: usize) {
    unsafe {
        HEAP_START = offset;
        OFFSET = 0; 
    }
}