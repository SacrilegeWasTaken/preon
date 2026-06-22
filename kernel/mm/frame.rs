use core::cell::UnsafeCell;

pub use kernel_arch::{PhysAddr, VirtAddr};

/// Page size in bytes. 4 KiB granule — must match the granule programmed
/// in `tcr::TCR_VALUE`.  
pub const PAGE_SIZE: usize = 4096;

const POOL_PAGES: usize = 32;
const POOL_SIZE: usize = POOL_PAGES * PAGE_SIZE;

/// Page-pool storage. Aligned to 4 KiB so every page handed out by               
/// [`alloc_page`] is naturally page-aligned.                   
#[repr(C, align(4096))]
struct PagePool(UnsafeCell<[u8; POOL_SIZE]>);

// Safety: concurrent access is serialised through `NEXT` — each CPU
// observes a unique slot index and writes only inside that slot.
unsafe impl Sync for PagePool {}

/// Bootstrap page pool. Lives in `.bss`, so it starts zeroed and pages           
/// handed out are already clean.                               
static POOL: PagePool = PagePool(UnsafeCell::new([0; POOL_SIZE]));

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::layout::image_va_to_pa;

/// Index of the next page to hand out. Lives in `.bss`, starts at 0.             
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Allocate a fresh, zero-filled 4 KiB physical page from the bootstrap pool.    
///
/// Pages are never freed — this allocator is only meant for boot-time            
/// page tables and similar lifelong bookkeeping.                                 
///
/// # Panics                                                                      
/// Panics if [`POOL_PAGES`] is exhausted. Bump it if early MMU bring-up
/// runs out of frames.                                                           
pub fn alloc_page() -> PhysAddr {
    let idx = NEXT.fetch_add(1, Ordering::Relaxed);
    if idx >= POOL_PAGES {
        panic!("bootstrap frame allocator exhausted ({} pages)", POOL_PAGES);
    }
    let base_va = VirtAddr::new(POOL.0.get() as usize);
    let base_pa = image_va_to_pa(base_va);
    PhysAddr::new(base_pa.as_usize() + idx * PAGE_SIZE)
}
