// memory allocator

use alloc::heap::{Alloc, AllocErr, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]

//heap_start and heap_end fields contain the start and end address of our kernel heap
//next contains the next free address and is increased every allocation
pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: AtomicUsize,
}

impl BumpAllocator {
    pub const fn new(heap_start: usize, heap_end: usize) -> Self {
        Self { heap_start, heap_end, next: AtomicUsize::new(heap_start) }
    }
}

unsafe impl<'a> Alloc for &'a BumpAllocator {

    //allocate a memory block
    unsafe fn alloc(&mut self, layout: Layout) -> Result<*mut u8, AllocErr>  {

        //if multiple threads try to allocate at the same time
        loop {
            //load current state of the `next` field
            let current_next = self.next.load(Ordering::Relaxed);
            //get start of memory
            let alloc_start = align_up(current_next, layout.align());
            //adds start address with the size
            let alloc_end = alloc_start.saturating_add(layout.size());

            //make sure size is below the end of the heap
            if alloc_end <= self.heap_end {
                //update the `next` pointer if it still has the value `current_next`
                //compare_and_swap - updates a variable with a given value if it still contains the value we expect
                //If it doesn't, it means that another thread updated the value simultaneously, so we need to try again
                let next_now = self.next.compare_and_swap(current_next, alloc_end, Ordering::Relaxed);
                if next_now == current_next {
                    // next address was successfully updated, allocation succeeded
                    return Ok(alloc_start as *mut u8);  //returns the allocated memory start address
                }
            } else {
                return Err(AllocErr::Exhausted{ request: layout })
            }
        }
    }
    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        // do nothing, leak memory
    }
}


/// Align downwards. Returns the greatest x with alignment `align`
/// so that x <= addr. The alignment must be a power of 2.
pub fn align_down(addr: usize, align: usize) -> usize {
    if align.is_power_of_two() {
        addr & !(align - 1)
    } else if align == 0 {
        addr
    } else {
        panic!("`align` must be a power of 2");
    }
}

/// Align upwards. Returns the smallest x with alignment `align`
/// so that x >= addr. The alignment must be a power of 2.
pub fn align_up(addr: usize, align: usize) -> usize {
    align_down(addr + align - 1, align)
}
