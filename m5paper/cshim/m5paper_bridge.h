#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint16_t panel_width;
    uint16_t panel_height;
    uint32_t image_buffer_addr;
    uint16_t vcom_mv;
} tern_m5paper_epd_info_t;

typedef struct {
    bool touched;
    uint16_t x;
    uint16_t y;
    uint16_t count;
} tern_m5paper_touch_state_t;

typedef struct {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t week;
    uint8_t hour;
    uint8_t minute;
    uint8_t second;
} tern_m5paper_rtc_datetime_t;

typedef enum {
    TERN_M5PAPER_OK = 0,
    TERN_M5PAPER_UNSUPPORTED = 1,
    TERN_M5PAPER_IO_ERROR = 2,
    TERN_M5PAPER_TIMEOUT = 3,
} tern_m5paper_status_t;

tern_m5paper_status_t tern_m5paper_board_init(void);
tern_m5paper_status_t tern_m5paper_epd_init(tern_m5paper_epd_info_t* out_info);
tern_m5paper_status_t tern_m5paper_epd_clear(bool init);
tern_m5paper_status_t tern_m5paper_epd_update_region(uint16_t x,
                                                     uint16_t y,
                                                     uint16_t width,
                                                     uint16_t height,
                                                     const uint8_t* data,
                                                     uint32_t data_len);
tern_m5paper_status_t tern_m5paper_touch_init(void);
tern_m5paper_status_t tern_m5paper_touch_read(tern_m5paper_touch_state_t* out_state);
tern_m5paper_status_t tern_m5paper_rtc_init(void);
tern_m5paper_status_t tern_m5paper_rtc_read(tern_m5paper_rtc_datetime_t* out_datetime);
tern_m5paper_status_t tern_m5paper_rtc_set(const tern_m5paper_rtc_datetime_t* datetime);

#ifdef __cplusplus
}
#endif
