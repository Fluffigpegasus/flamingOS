//temporary mapping of pages
//we need to zero the P4 table since it can map garbage otherwise but we can't zero it right now because the p4_frame is not mapped to a virtual address


use super::Page;
use super::{ActivePageTable, VirtualAddress};
use super::table::{Table, Level1};
use memory::Frame;

pub struct TemporaryPage {
    page: Page,
    allocator: TinyAllocator,
}

impl TemporaryPage {


    pub fn new<A>(page: Page, allocator: &mut A) -> TemporaryPage
        where A: FrameAllocator
    {
        TemporaryPage {
            page: page,
            allocator: TinyAllocator::new(allocator),
        }
    }

    /// Maps the temporary page to the given frame in the active table.
    /// Returns the start address of the temporary page.
    pub fn map(&mut self, frame: Frame, active_table: &mut ActivePageTable)
               -> VirtualAddress
    {
        use super::entry::WRITABLE;

        assert!(active_table.translate_page(self.page).is_none(),
                "temporary page is already mapped");
        active_table.map_to(self.page, frame, WRITABLE, &mut self.allocator);
        self.page.start_address()
    }

    /// Unmaps the temporary page in the active table.
    pub fn unmap(&mut self, active_table: &mut ActivePageTable) {
        active_table.unmap(self.page, &mut self.allocator)
    }

    /// Maps the temporary page to the given page table frame in the active
    /// table. Returns a reference to the now mapped table.
    // interprets the given frame as a page table and returns a Table reference
    //we return table one since it forbids calling the next_table methods
    pub fn map_table_frame(&mut self, frame: Frame, active_table: &mut ActivePageTable) -> &mut Table<Level1>
    {
        unsafe { &mut *(self.map(frame, active_table) as *mut Table<Level1>) }
    }
}

//allocator only holds 3 frames, P3, P2 and P1
//P4 is always mapped since we start from there when we need to access a page
struct TinyAllocator([Option<Frame>; 3]);

impl TinyAllocator {

    fn new<A>(allocator: &mut A) -> TinyAllocator
        where A: FrameAllocator
    {
        let mut f = || allocator.allocate_frame();
        let frames = [f(), f(), f()];
        TinyAllocator(frames)
    }
}

use memory::FrameAllocator;

// make our tinyallocator to a frameAllocator
// that can allocate and deallocate frame
impl FrameAllocator for TinyAllocator {

    fn allocate_frame(&mut self) -> Option<Frame> {
        for frame_option in &mut self.0 {
            if frame_option.is_some() {
                return frame_option.take();
            }
        }
        None
    }

    fn deallocate_frame(&mut self, frame: Frame) {
        for frame_option in &mut self.0 {
            if frame_option.is_none() {
                *frame_option = Some(frame);
                return;
            }
        }
        panic!("Tiny allocator can hold only 3 frames.");
    }
}
