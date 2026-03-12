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
    bool up_pressed;
    bool power_pressed;
    bool down_pressed;
} tern_m5paper_button_state_t;

typedef enum {
    TERN_M5PAPER_INPUT_NONE = 0,
    TERN_M5PAPER_INPUT_BUTTON_DOWN = 1,
    TERN_M5PAPER_INPUT_BUTTON_UP = 2,
    TERN_M5PAPER_INPUT_TOUCH_DOWN = 3,
    TERN_M5PAPER_INPUT_TOUCH_MOVE = 4,
    TERN_M5PAPER_INPUT_TOUCH_UP = 5,
} tern_m5paper_input_event_type_t;

typedef enum {
    TERN_M5PAPER_BUTTON_UP = 1,
    TERN_M5PAPER_BUTTON_DOWN = 2,
    TERN_M5PAPER_BUTTON_POWER = 3,
} tern_m5paper_button_id_t;

typedef struct {
    uint8_t event_type;
    uint8_t button_id;
    uint16_t x;
    uint16_t y;
    uint16_t touch_count;
} tern_m5paper_input_event_t;

typedef struct {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t week;
    uint8_t hour;
    uint8_t minute;
    uint8_t second;
} tern_m5paper_rtc_datetime_t;

typedef struct {
    bool is_dir;
    uint32_t size;
    char name[256];
} tern_m5paper_storage_entry_t;

typedef enum {
    TERN_M5PAPER_OK = 0,
    TERN_M5PAPER_UNSUPPORTED = 1,
    TERN_M5PAPER_IO_ERROR = 2,
    TERN_M5PAPER_TIMEOUT = 3,
    TERN_M5PAPER_NOT_FOUND = 4,
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
tern_m5paper_status_t tern_m5paper_buttons_read(tern_m5paper_button_state_t* out_state);
tern_m5paper_status_t tern_m5paper_input_next(tern_m5paper_input_event_t* out_event);
tern_m5paper_status_t tern_m5paper_rtc_init(void);
tern_m5paper_status_t tern_m5paper_rtc_read(tern_m5paper_rtc_datetime_t* out_datetime);
tern_m5paper_status_t tern_m5paper_rtc_set(const tern_m5paper_rtc_datetime_t* datetime);
tern_m5paper_status_t tern_m5paper_storage_init(void);
bool tern_m5paper_storage_exists(const char* path);
tern_m5paper_status_t tern_m5paper_storage_list_begin(const char* path);
tern_m5paper_status_t tern_m5paper_storage_list_next(tern_m5paper_storage_entry_t* out_entry);
void tern_m5paper_storage_list_end(void);
tern_m5paper_status_t tern_m5paper_storage_file_size(const char* path, uint32_t* out_size);
tern_m5paper_status_t tern_m5paper_storage_read_chunk(const char* path,
                                                      uint32_t offset,
                                                      uint8_t* out_buf,
                                                      uint32_t buf_len,
                                                      uint32_t* out_read);

#ifdef __cplusplus
}
#endif
