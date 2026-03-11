#include "m5paper_bridge.h"

tern_m5paper_status_t tern_m5paper_board_init(void) {
    return TERN_M5PAPER_UNSUPPORTED;
}

tern_m5paper_status_t tern_m5paper_epd_init(tern_m5paper_epd_info_t* out_info) {
    if (out_info) {
        out_info->panel_width = 0;
        out_info->panel_height = 0;
        out_info->image_buffer_addr = 0;
        out_info->vcom_mv = 0;
    }
    return TERN_M5PAPER_UNSUPPORTED;
}

tern_m5paper_status_t tern_m5paper_epd_clear(bool init) {
    (void)init;
    return TERN_M5PAPER_UNSUPPORTED;
}

tern_m5paper_status_t tern_m5paper_epd_update_region(uint16_t x,
                                                     uint16_t y,
                                                     uint16_t width,
                                                     uint16_t height,
                                                     const uint8_t* data,
                                                     uint32_t data_len) {
    (void)x;
    (void)y;
    (void)width;
    (void)height;
    (void)data;
    (void)data_len;
    return TERN_M5PAPER_UNSUPPORTED;
}

tern_m5paper_status_t tern_m5paper_touch_init(void) {
    return TERN_M5PAPER_UNSUPPORTED;
}

tern_m5paper_status_t tern_m5paper_touch_read(tern_m5paper_touch_state_t* out_state) {
    if (out_state) {
        out_state->touched = false;
        out_state->x = 0;
        out_state->y = 0;
        out_state->count = 0;
    }
    return TERN_M5PAPER_UNSUPPORTED;
}
