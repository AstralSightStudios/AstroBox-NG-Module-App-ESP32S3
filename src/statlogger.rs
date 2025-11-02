use esp_idf_svc::sys::{heap_caps_get_info, multi_heap_info_t, MALLOC_CAP_DEFAULT};
use log::info;

pub fn log_heap_info() {
    unsafe {
        let mut info: multi_heap_info_t = core::mem::zeroed();

        heap_caps_get_info(&mut info as *mut _, MALLOC_CAP_DEFAULT as u32);

        info!("===== Heap Info (MALLOC_CAP_DEFAULT) =====");
        info!("Total:    {} bytes", info.total_free_bytes);
        info!("Free:     {} bytes", info.total_free_bytes);
        info!("Min free: {} bytes", info.minimum_free_bytes);
        info!("Largest:  {} bytes", info.largest_free_block);
        info!("Alloc blk:{} bytes", info.total_allocated_bytes);
    }
}
