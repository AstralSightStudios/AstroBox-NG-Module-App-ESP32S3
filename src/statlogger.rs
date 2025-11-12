use esp_idf_svc::sys::{
    heap_caps_get_free_size, MALLOC_CAP_8BIT, MALLOC_CAP_DMA, MALLOC_CAP_INTERNAL,
    MALLOC_CAP_SPIRAM,
};
use log::info;

pub fn log_heap_info() {
    unsafe {
        let free_int = heap_caps_get_free_size(MALLOC_CAP_INTERNAL as u32);
        let free_dma = heap_caps_get_free_size(MALLOC_CAP_DMA as u32);
        let free_8bit = heap_caps_get_free_size(MALLOC_CAP_8BIT as u32);
        let free_ps = heap_caps_get_free_size(MALLOC_CAP_SPIRAM as u32);
        info!(
            "FREE internal={} dma={} 8bit={} psram={}",
            free_int, free_dma, free_8bit, free_ps
        );
    }
}
