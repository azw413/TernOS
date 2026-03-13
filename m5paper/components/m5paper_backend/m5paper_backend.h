#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum tern_m5paper_backend_status_t {
    TERN_M5PAPER_BACKEND_OK = 0,
    TERN_M5PAPER_BACKEND_IO_ERROR = 1,
    TERN_M5PAPER_BACKEND_TIMEOUT = 2,
    TERN_M5PAPER_BACKEND_NOT_FOUND = 3,
} tern_m5paper_backend_status_t;

typedef struct tern_m5paper_backend_epd_info_t {
    uint16_t panel_width;
    uint16_t panel_height;
    uint32_t image_buffer_addr;
    uint16_t vcom_mv;
} tern_m5paper_backend_epd_info_t;

typedef enum tern_m5paper_backend_update_mode_t {
    TERN_M5PAPER_BACKEND_UPDATE_FAST = 0,
    TERN_M5PAPER_BACKEND_UPDATE_QUALITY = 1,
} tern_m5paper_backend_update_mode_t;

typedef struct tern_m5paper_backend_touch_state_t {
    bool touched;
    uint16_t x;
    uint16_t y;
    uint16_t count;
} tern_m5paper_backend_touch_state_t;

typedef struct tern_m5paper_backend_button_state_t {
    bool up_pressed;
    bool power_pressed;
    bool down_pressed;
} tern_m5paper_backend_button_state_t;

typedef enum tern_m5paper_backend_input_event_type_t {
    TERN_M5PAPER_BACKEND_INPUT_NONE = 0,
    TERN_M5PAPER_BACKEND_INPUT_BUTTON_DOWN = 1,
    TERN_M5PAPER_BACKEND_INPUT_BUTTON_UP = 2,
    TERN_M5PAPER_BACKEND_INPUT_TOUCH_DOWN = 3,
    TERN_M5PAPER_BACKEND_INPUT_TOUCH_MOVE = 4,
    TERN_M5PAPER_BACKEND_INPUT_TOUCH_UP = 5,
} tern_m5paper_backend_input_event_type_t;

typedef enum tern_m5paper_backend_button_id_t {
    TERN_M5PAPER_BACKEND_BUTTON_UP = 1,
    TERN_M5PAPER_BACKEND_BUTTON_DOWN = 2,
    TERN_M5PAPER_BACKEND_BUTTON_POWER = 3,
} tern_m5paper_backend_button_id_t;

typedef struct tern_m5paper_backend_input_event_t {
    uint8_t event_type;
    uint8_t button_id;
    uint16_t x;
    uint16_t y;
    uint16_t touch_count;
} tern_m5paper_backend_input_event_t;

typedef struct tern_m5paper_backend_rtc_datetime_t {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t week;
    uint8_t hour;
    uint8_t minute;
    uint8_t second;
} tern_m5paper_backend_rtc_datetime_t;

typedef struct tern_m5paper_backend_storage_entry_t {
    bool is_dir;
    uint32_t size;
    char name[256];
} tern_m5paper_backend_storage_entry_t;

typedef struct tern_m5paper_backend_battery_status_t {
    int32_t percent;
    uint32_t millivolts;
    bool charging;
} tern_m5paper_backend_battery_status_t;

tern_m5paper_backend_status_t tern_m5paper_backend_board_init(void);
tern_m5paper_backend_status_t tern_m5paper_backend_start(void);
tern_m5paper_backend_status_t tern_m5paper_backend_epd_init(tern_m5paper_backend_epd_info_t* out_info);
tern_m5paper_backend_status_t tern_m5paper_backend_epd_clear(bool init);
tern_m5paper_backend_status_t tern_m5paper_backend_epd_fill_white(void);
tern_m5paper_backend_status_t tern_m5paper_backend_epd_update_region(uint16_t x, uint16_t y, uint16_t width, uint16_t height, const uint8_t* data, uint32_t data_len, tern_m5paper_backend_update_mode_t mode);
tern_m5paper_backend_status_t tern_m5paper_backend_epd_test_pattern(uint16_t x, uint16_t y, uint16_t width, uint16_t height);
tern_m5paper_backend_status_t tern_m5paper_backend_touch_init(void);
tern_m5paper_backend_status_t tern_m5paper_backend_touch_read(tern_m5paper_backend_touch_state_t* out_state);
tern_m5paper_backend_status_t tern_m5paper_backend_buttons_read(tern_m5paper_backend_button_state_t* out_state);
tern_m5paper_backend_status_t tern_m5paper_backend_input_next(tern_m5paper_backend_input_event_t* out_event);
tern_m5paper_backend_status_t tern_m5paper_backend_rtc_init(void);
tern_m5paper_backend_status_t tern_m5paper_backend_rtc_read(tern_m5paper_backend_rtc_datetime_t* out_datetime);
tern_m5paper_backend_status_t tern_m5paper_backend_rtc_set(const tern_m5paper_backend_rtc_datetime_t* datetime);
tern_m5paper_backend_status_t tern_m5paper_backend_battery_read(tern_m5paper_backend_battery_status_t* out_status);
tern_m5paper_backend_status_t tern_m5paper_backend_sleep(bool deep);
tern_m5paper_backend_status_t tern_m5paper_backend_storage_init(void);
bool tern_m5paper_backend_storage_exists(const char* path);
tern_m5paper_backend_status_t tern_m5paper_backend_storage_list_begin(const char* path);
tern_m5paper_backend_status_t tern_m5paper_backend_storage_list_next(tern_m5paper_backend_storage_entry_t* out_entry);
void tern_m5paper_backend_storage_list_end(void);
tern_m5paper_backend_status_t tern_m5paper_backend_storage_file_size(const char* path, uint32_t* out_size);
tern_m5paper_backend_status_t tern_m5paper_backend_storage_read_chunk(const char* path, uint32_t offset, uint8_t* out_buf, uint32_t buf_len, uint32_t* out_read);

#ifdef __cplusplus
}
#endif
