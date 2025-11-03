use esp_idf_svc::sys::esp_get_free_heap_size;
use log::info;

pub fn log_heap_info() {
    unsafe {
        let free_heap_size = esp_get_free_heap_size();
        info!("Free heap size: {} bytes", free_heap_size);
    }
}
