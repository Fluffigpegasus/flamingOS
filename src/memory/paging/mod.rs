// paging module

pub use self::entry::*;     //export for all entry types
use core::ptr::Unique;
use memory::FrameAllocator;
use self::table::{Table, Level4};
use memory::PAGE_SIZE;
use memory::Frame;
use memory::paging::table::P4;

mod entry;
mod table;


const ENTRY_COUNT: usize = 512;     // number of entries per table

pub type PhysicalAddress = usize;
pub type VirtualAddress = usize;


#[derive(Debug, Clone, Copy)]
pub struct Page {
    number: usize,
}

impl Page {

    // get the Page from the virtual address
    pub fn containing_address(address: VirtualAddress) -> Page {
        // make sure we do not access a invalid virtual adress
        // address space is split up into two halves, one with sign extension adresses and one without
        // everything in between is invalid -> invalid address
        assert!(address < 0x0000_8000_0000_0000 ||
                address >= 0xffff_8000_0000_0000,
                "invalid address: 0x{:x}", address);
        Page { number: address / PAGE_SIZE }
    }

    fn start_address(&self) -> usize {
        self.number * PAGE_SIZE
    }

    // returns the different table indexes
    fn p4_index(&self) -> usize {
        (self.number >> 27) & 0o777
    }
    fn p3_index(&self) -> usize {
        (self.number >> 18) & 0o777
    }
    fn p2_index(&self) -> usize {
        (self.number >> 9) & 0o777
    }
    fn p1_index(&self) -> usize {
        (self.number >> 0) & 0o777
    }
}

// P4 table is owned by the ActivePageTable struct
// use unique to indicate ownership
pub struct ActivePageTable {
    p4: Unique<Table<Level4>>,
}
impl ActivePageTable {

    pub unsafe fn new() -> ActivePageTable {
        ActivePageTable {
            // create a new Unique
            p4: Unique::new_unchecked(table::P4),
        }
    }

    // methods for references to the P4 table
    fn p4(&self) -> &Table<Level4> {
        unsafe { self.p4.as_ref() }
    }
    fn p4_mut(&mut self) -> &mut Table<Level4> {
        unsafe { self.p4.as_mut() }
    }

    // translates virtual address to physical address
    /// Returns `None` if the address is not mapped.
    pub fn translate(&self, virtual_address: VirtualAddress) -> Option<PhysicalAddress> {
        let offset = virtual_address % PAGE_SIZE;

        self.translate_page(Page::containing_address(virtual_address)).map(|frame| frame.number * PAGE_SIZE + offset)
    }

    // takes a page and returns the corresponding frame
    fn translate_page(&self, page: Page) -> Option<Frame> {
        use self::entry::HUGE_PAGE;

        // unsafe to convert the P4 pointer to a reference
        let p3 = self.p4().next_table(page.p4_index());

        // calculates corresponding frame if huge pages are used
        let huge_page = || {
            p3.and_then(|p3| {
                let p3_entry = &p3[page.p3_index()];
                // 1GiB page?
                if let Some(start_frame) = p3_entry.pointed_frame() {
                    if p3_entry.flags().contains(HUGE_PAGE) {
                        // address must be 1GiB aligned
                        assert!(start_frame.number % (ENTRY_COUNT * ENTRY_COUNT) == 0);
                        return Some(Frame {
                            number: start_frame.number + page.p2_index() *
                                ENTRY_COUNT + page.p1_index(),
                        });
                    }
                }
                if let Some(p2) = p3.next_table(page.p3_index()) {
                    let p2_entry = &p2[page.p2_index()];
                    // 2MiB page?
                    if let Some(start_frame) = p2_entry.pointed_frame() {
                        if p2_entry.flags().contains(HUGE_PAGE) {
                            // address must be 2MiB aligned
                            assert!(start_frame.number % ENTRY_COUNT == 0);
                            return Some(Frame {
                                number: start_frame.number + page.p1_index()
                            });
                        }
                    }
                }
                None
            })
        };

        // use the and_then function to go through the four table levels to find the frame
        // if some entry is None, we check if the page is a huge page
        p3.and_then(|p3| p3.next_table(page.p3_index()))
            .and_then(|p2| p2.next_table(page.p2_index()))
            .and_then(|p1| p1[page.p1_index()].pointed_frame())
            .or_else(huge_page)
    }

    // map a page to a frame
    /// The `PRESENT` flag is added by default. Needs a
    /// `FrameAllocator` as it might need to create new page tables
    pub fn map_to<A>(&mut self, page: Page, frame: Frame, flags: EntryFlags, allocator: &mut A)
        where A: FrameAllocator
    {

        // return next table if it exist or create a new one
        let p4 = self.p4_mut();
        let mut p3 = p4.next_table_create(page.p4_index(), allocator);
        let mut p2 = p3.next_table_create(page.p3_index(), allocator);
        let mut p1 = p2.next_table_create(page.p2_index(), allocator);

        // assert that the page is unmapped and set the present flag
        assert!(p1[page.p1_index()].is_unused());
        p1[page.p1_index()].set(frame, flags | PRESENT);
    }

    // method that just picks a free frame for us
    /// Maps the page to some free frame with the provided flags.
    /// The free frame is allocated from the given `FrameAllocator`.
    pub fn map<A>(&mut self, page: Page, flags: EntryFlags, allocator: &mut A)
        where A: FrameAllocator
    {
        let frame = allocator.allocate_frame().expect("out of memory");
        self.map_to(page, frame, flags, allocator)
    }

    // identity mapping to make it easier to remap the kernel
    /// Identity map the the given frame with the provided flags.
    /// The `FrameAllocator` is used to create new page tables if needed.
    pub fn identity_map<A>(&mut self, frame: Frame, flags: EntryFlags, allocator: &mut A)
        where A: FrameAllocator
    {
        let page = Page::containing_address(frame.start_address());
        self.map_to(page, frame, flags, allocator)
    }

    // to unmap a page we set the corresponding P1 entry to unused
    /// Unmaps the given page and adds all freed frames to the given
    /// `FrameAllocator`.
    fn unmap<A>(&mut self, page: Page, allocator: &mut A)
        where A: FrameAllocator
    {


        assert!(self.translate(page.start_address()).is_some());

        let p1 = self.p4_mut()
            .next_table_mut(page.p4_index())
            .and_then(|p3| p3.next_table_mut(page.p3_index()))
            .and_then(|p2| p2.next_table_mut(page.p2_index()))
            .expect("mapping code does not support huge pages");

        let frame = p1[page.p1_index()].pointed_frame().unwrap();
        p1[page.p1_index()].set_unused();

        use x86_64::instructions::tlb;
        use x86_64::VirtualAddress;
        tlb::flush(VirtualAddress(page.start_address()));
        // TODO free p(1,2,3) table if empty
        //allocator.deallocate_frame(frame);
    }
}



// function to test the paging
pub fn test_paging<A>(allocator: &mut A)
    where A: FrameAllocator
{
    let mut page_table = unsafe { ActivePageTable::new() };

    let addr = 42 * 512 * 512 * 4096; // 42th P3 entry
    let page = Page::containing_address(addr);
    let frame = allocator.allocate_frame().expect("no more frames");

    println!("None = {:?}, map to {:?}", page_table.translate(addr),frame);

    page_table.map_to(page, frame, EntryFlags::empty(), allocator);

    println!("Some = {:?}", page_table.translate(addr));
    println!("next free frame: {:?}", allocator.allocate_frame());

    page_table.unmap(Page::containing_address(addr), allocator);
    println!("None = {:?}", page_table.translate(addr));

    println!("{:#x}", unsafe {
        *(Page::containing_address(addr).start_address() as *const u64)
    });
}
