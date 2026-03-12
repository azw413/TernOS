#include "m5paper_backend.h"

#include <Arduino.h>
#include <SPI.h>
#include <SD.h>
#include <Wire.h>
#include <cstdlib>
#include <array>
#include <cstring>
#include <dirent.h>
#include <string>
#include <sys/stat.h>

#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#include "M5EPD_Driver.h"
#include "BM8563.h"
#include "GT911.h"

namespace {

constexpr int kMainPowerPin = 2;
constexpr int kExtPowerPin = 5;
constexpr int kEpdPowerPin = 23;
constexpr int kEpdCsPin = 15;
constexpr int kEpdSckPin = 14;
constexpr int kEpdMosiPin = 12;
constexpr int kEpdMisoPin = 13;
constexpr int kEpdBusyPin = 27;
constexpr int kTouchIntPin = 36;
constexpr int kTouchSdaPin = 21;
constexpr int kTouchSclPin = 22;
constexpr int kButtonUpPin = 37;
constexpr int kButtonPowerPin = 38;
constexpr int kButtonDownPin = 39;

constexpr uint16_t kPanelWidth = 540;
constexpr uint16_t kPanelHeight = 960;
constexpr uint32_t kImageBufferAddr = 0x001236E0;
constexpr uint16_t kVcomMv = 2300;
constexpr const char* kStorageMountPoint = "/sd";
constexpr const char* kLogTag = "m5paper_backend";
constexpr uint32_t kTaskStackWords = 8 * 1024;
constexpr UBaseType_t kTaskPriority = 5;
constexpr size_t kInputQueueCapacity = 64;

bool g_arduino_ready = false;
bool g_board_ready = false;
bool g_epd_ready = false;
bool g_touch_ready = false;
bool g_rtc_ready = false;
bool g_storage_ready = false;
bool g_started = false;
M5EPD_Driver g_epd(VSPI);
BM8563 g_rtc;
GT911 g_touch;
DIR* g_storage_list_dir = nullptr;
char g_storage_list_path[384] = {};
std::array<tern_m5paper_backend_input_event_t, kInputQueueCapacity> g_input_queue = {};
size_t g_input_queue_head = 0;
size_t g_input_queue_tail = 0;

tern_m5paper_backend_status_t map_epd_status(m5epd_err_t status) {
    switch (status) {
        case M5EPD_OK: return TERN_M5PAPER_BACKEND_OK;
        case M5EPD_BUSYTIMEOUT: return TERN_M5PAPER_BACKEND_TIMEOUT;
        default: return TERN_M5PAPER_BACKEND_IO_ERROR;
    }
}

void ensure_arduino() {
    if (!g_arduino_ready) {
        initArduino();
        g_arduino_ready = true;
    }
}

bool normalize_storage_path(const char* path, char* out_path, size_t out_len) {
    if (out_path == nullptr || out_len == 0) return false;
    if (path == nullptr || path[0] == '\0' || std::strcmp(path, "/") == 0) {
        return std::snprintf(out_path, out_len, "%s", kStorageMountPoint) < static_cast<int>(out_len);
    }
    if (path[0] == '/') {
        return std::snprintf(out_path, out_len, "%s%s", kStorageMountPoint, path) < static_cast<int>(out_len);
    }
    return std::snprintf(out_path, out_len, "%s/%s", kStorageMountPoint, path) < static_cast<int>(out_len);
}

tern_m5paper_backend_status_t stat_storage_path(const char* path, struct stat* st) {
    char full_path[384];
    if (!normalize_storage_path(path, full_path, sizeof(full_path))) return TERN_M5PAPER_BACKEND_IO_ERROR;
    if (stat(full_path, st) != 0) return TERN_M5PAPER_BACKEND_NOT_FOUND;
    return TERN_M5PAPER_BACKEND_OK;
}

void push_input_event(const tern_m5paper_backend_input_event_t& event) {
    const size_t next_tail = (g_input_queue_tail + 1) % kInputQueueCapacity;
    if (next_tail == g_input_queue_head) {
        g_input_queue_head = (g_input_queue_head + 1) % kInputQueueCapacity;
    }
    g_input_queue[g_input_queue_tail] = event;
    g_input_queue_tail = next_tail;
}

void backend_task(void*) {
    tern_m5paper_backend_button_state_t last_buttons = {};
    tern_m5paper_backend_touch_state_t last_touch = {};
    while (true) {
        tern_m5paper_backend_button_state_t buttons = {};
        if (tern_m5paper_backend_buttons_read(&buttons) == TERN_M5PAPER_BACKEND_OK) {
            if (buttons.up_pressed != last_buttons.up_pressed) {
                push_input_event({static_cast<uint8_t>(buttons.up_pressed ? TERN_M5PAPER_BACKEND_INPUT_BUTTON_DOWN : TERN_M5PAPER_BACKEND_INPUT_BUTTON_UP), TERN_M5PAPER_BACKEND_BUTTON_UP, 0, 0, 0});
            }
            if (buttons.power_pressed != last_buttons.power_pressed) {
                push_input_event({static_cast<uint8_t>(buttons.power_pressed ? TERN_M5PAPER_BACKEND_INPUT_BUTTON_DOWN : TERN_M5PAPER_BACKEND_INPUT_BUTTON_UP), TERN_M5PAPER_BACKEND_BUTTON_POWER, 0, 0, 0});
            }
            if (buttons.down_pressed != last_buttons.down_pressed) {
                push_input_event({static_cast<uint8_t>(buttons.down_pressed ? TERN_M5PAPER_BACKEND_INPUT_BUTTON_DOWN : TERN_M5PAPER_BACKEND_INPUT_BUTTON_UP), TERN_M5PAPER_BACKEND_BUTTON_DOWN, 0, 0, 0});
            }
            last_buttons = buttons;
        }

        tern_m5paper_backend_touch_state_t touch = {};
        if (tern_m5paper_backend_touch_read(&touch) == TERN_M5PAPER_BACKEND_OK) {
            if (touch.touched != last_touch.touched) {
                push_input_event({static_cast<uint8_t>(touch.touched ? TERN_M5PAPER_BACKEND_INPUT_TOUCH_DOWN : TERN_M5PAPER_BACKEND_INPUT_TOUCH_UP), 0, touch.x, touch.y, touch.count});
                last_touch = touch;
            } else if (touch.touched && (touch.x != last_touch.x || touch.y != last_touch.y || touch.count != last_touch.count)) {
                push_input_event({TERN_M5PAPER_BACKEND_INPUT_TOUCH_MOVE, 0, touch.x, touch.y, touch.count});
                last_touch = touch;
            }
        }

        vTaskDelay(pdMS_TO_TICKS(20));
    }
}

}  // namespace

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_board_init(void) {
    ensure_arduino();
    if (g_board_ready) return TERN_M5PAPER_BACKEND_OK;
    pinMode(kMainPowerPin, OUTPUT); digitalWrite(kMainPowerPin, HIGH);
    pinMode(kExtPowerPin, OUTPUT); digitalWrite(kExtPowerPin, HIGH);
    pinMode(kEpdPowerPin, OUTPUT); digitalWrite(kEpdPowerPin, HIGH);
    pinMode(kButtonUpPin, INPUT_PULLUP);
    pinMode(kButtonPowerPin, INPUT_PULLUP);
    pinMode(kButtonDownPin, INPUT_PULLUP);
    delay(1000);
    g_board_ready = true;
    ESP_LOGI(kLogTag, "board_init ok");
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_start(void) {
    if (g_started) return TERN_M5PAPER_BACKEND_OK;
    auto status = tern_m5paper_backend_board_init();
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    status = tern_m5paper_backend_touch_init();
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    BaseType_t ok = xTaskCreatePinnedToCore(backend_task, "m5paper_backend", kTaskStackWords, nullptr, kTaskPriority, nullptr, 1);
    if (ok != pdPASS) return TERN_M5PAPER_BACKEND_IO_ERROR;
    g_started = true;
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_epd_init(tern_m5paper_backend_epd_info_t* out_info) {
    if (!g_board_ready) {
        auto status = tern_m5paper_backend_board_init();
        if (status != TERN_M5PAPER_BACKEND_OK) return status;
    }
    auto status = map_epd_status(g_epd.begin(kEpdSckPin, kEpdMosiPin, kEpdMisoPin, kEpdCsPin, kEpdBusyPin, -1));
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    g_epd.SetRotation(M5EPD_Driver::ROTATE_90);
    g_epd_ready = true;
    if (out_info) {
        out_info->panel_width = kPanelWidth;
        out_info->panel_height = kPanelHeight;
        out_info->image_buffer_addr = kImageBufferAddr;
        out_info->vcom_mv = kVcomMv;
    }
    ESP_LOGI(kLogTag, "epd_init ok");
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_epd_clear(bool init) {
    if (!g_epd_ready) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = map_epd_status(g_epd.Clear(init));
    ESP_LOGI(kLogTag, "epd_clear=%d", static_cast<int>(status));
    return status;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_epd_fill_white(void) {
    if (!g_epd_ready) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = map_epd_status(g_epd.Clear(true));
    ESP_LOGI(kLogTag, "epd_fill_white=%d", static_cast<int>(status));
    return status;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_epd_update_region(uint16_t x, uint16_t y, uint16_t width, uint16_t height, const uint8_t* data, uint32_t data_len) {
    if (!g_epd_ready || data == nullptr) return TERN_M5PAPER_BACKEND_IO_ERROR;
    const uint32_t expected = (static_cast<uint32_t>(width) * static_cast<uint32_t>(height)) / 2;
    if (data_len < expected) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = map_epd_status(g_epd.WritePartGram4bpp(x, y, width, height, data));
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    status = map_epd_status(g_epd.UpdateArea(x, y, width, height, UPDATE_MODE_GC16));
    ESP_LOGI(kLogTag, "epd_update_region=%d x=%u y=%u w=%u h=%u", static_cast<int>(status), x, y, width, height);
    return status;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_epd_test_pattern(uint16_t x, uint16_t y, uint16_t width, uint16_t height) {
    if (!g_epd_ready) return TERN_M5PAPER_BACKEND_IO_ERROR;
    x = 0;
    y = 0;
    width = kPanelWidth;
    height = kPanelHeight;
    const uint16_t half = height / 2;
    auto status = map_epd_status(g_epd.FillPartGram4bpp(x, y, width, half, 0x0000));
    ESP_LOGI(kLogTag, "epd_test_pattern top_fill=%d value=0x0000", static_cast<int>(status));
    if (status != TERN_M5PAPER_BACKEND_OK) {
        return status;
    }
    status = map_epd_status(g_epd.FillPartGram4bpp(x, y + half, width, height - half, 0xFFFF));
    ESP_LOGI(kLogTag, "epd_test_pattern bottom_fill=%d value=0xFFFF", static_cast<int>(status));
    if (status != TERN_M5PAPER_BACKEND_OK) {
        return status;
    }
    status = map_epd_status(g_epd.UpdateArea(x, y, width, height, UPDATE_MODE_GC16));
    ESP_LOGI(kLogTag, "epd_test_pattern=%d x=%u y=%u w=%u h=%u", static_cast<int>(status), x, y, width, height);
    return status;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_touch_init(void) {
    auto board_status = tern_m5paper_backend_board_init();
    if (board_status != TERN_M5PAPER_BACKEND_OK) return board_status;
    if (!g_touch_ready) {
        if (g_touch.begin(kTouchSdaPin, kTouchSclPin, kTouchIntPin) != ESP_OK) return TERN_M5PAPER_BACKEND_IO_ERROR;
        g_touch.SetRotation(GT911::ROTATE_90);
        g_touch_ready = true;
        ESP_LOGI(kLogTag, "touch_init ok");
    }
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_touch_read(tern_m5paper_backend_touch_state_t* out_state) {
    if (!out_state) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = tern_m5paper_backend_touch_init();
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    if (g_touch.available()) g_touch.update();
    out_state->count = g_touch.getFingerNum();
    out_state->touched = out_state->count > 0;
    out_state->x = out_state->touched ? g_touch.readFingerX(0) : 0;
    out_state->y = out_state->touched ? g_touch.readFingerY(0) : 0;
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_buttons_read(tern_m5paper_backend_button_state_t* out_state) {
    if (!out_state) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = tern_m5paper_backend_board_init();
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    out_state->up_pressed = digitalRead(kButtonUpPin) == LOW;
    out_state->power_pressed = digitalRead(kButtonPowerPin) == LOW;
    out_state->down_pressed = digitalRead(kButtonDownPin) == LOW;
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_input_next(tern_m5paper_backend_input_event_t* out_event) {
    if (!out_event) return TERN_M5PAPER_BACKEND_IO_ERROR;
    if (g_input_queue_head == g_input_queue_tail) return TERN_M5PAPER_BACKEND_NOT_FOUND;
    *out_event = g_input_queue[g_input_queue_head];
    g_input_queue_head = (g_input_queue_head + 1) % kInputQueueCapacity;
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_rtc_init(void) {
    auto board_status = tern_m5paper_backend_board_init();
    if (board_status != TERN_M5PAPER_BACKEND_OK) return board_status;
    if (!g_rtc_ready) {
        g_rtc.begin();
        g_rtc_ready = true;
    }
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_rtc_read(tern_m5paper_backend_rtc_datetime_t* out_datetime) {
    if (!out_datetime) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = tern_m5paper_backend_rtc_init();
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    rtc_date_t date; rtc_time_t time;
    g_rtc.getDate(&date); g_rtc.getTime(&time);
    out_datetime->year = static_cast<uint16_t>(date.year);
    out_datetime->month = static_cast<uint8_t>(date.mon);
    out_datetime->day = static_cast<uint8_t>(date.day);
    out_datetime->week = static_cast<uint8_t>(date.week);
    out_datetime->hour = static_cast<uint8_t>(time.hour);
    out_datetime->minute = static_cast<uint8_t>(time.min);
    out_datetime->second = static_cast<uint8_t>(time.sec);
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_rtc_set(const tern_m5paper_backend_rtc_datetime_t* datetime) {
    if (!datetime) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = tern_m5paper_backend_rtc_init();
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    rtc_date_t date{static_cast<int8_t>(datetime->week), static_cast<int8_t>(datetime->month), static_cast<int8_t>(datetime->day), static_cast<int16_t>(datetime->year)};
    rtc_time_t time{static_cast<int8_t>(datetime->hour), static_cast<int8_t>(datetime->minute), static_cast<int8_t>(datetime->second)};
    if (!g_rtc.setDate(&date) || !g_rtc.setTime(&time)) return TERN_M5PAPER_BACKEND_IO_ERROR;
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_storage_init(void) {
    auto board_status = tern_m5paper_backend_board_init();
    if (board_status != TERN_M5PAPER_BACKEND_OK) return board_status;
    auto epd_status = tern_m5paper_backend_epd_init(nullptr);
    if (epd_status != TERN_M5PAPER_BACKEND_OK) return epd_status;
    if (!g_storage_ready) {
        if (!SD.begin(4, *g_epd.GetSPI(), 20000000, kStorageMountPoint, 8, false)) return TERN_M5PAPER_BACKEND_IO_ERROR;
        g_storage_ready = true;
    }
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" bool tern_m5paper_backend_storage_exists(const char* path) {
    if (!path || tern_m5paper_backend_storage_init() != TERN_M5PAPER_BACKEND_OK) return false;
    struct stat st = {};
    return stat_storage_path(path, &st) == TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_storage_list_begin(const char* path) {
    if (!path) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = tern_m5paper_backend_storage_init();
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    if (g_storage_list_dir) {
        closedir(g_storage_list_dir);
        g_storage_list_dir = nullptr;
    }
    char full_path[384];
    if (!normalize_storage_path(path, full_path, sizeof(full_path))) return TERN_M5PAPER_BACKEND_IO_ERROR;
    g_storage_list_dir = opendir(full_path);
    if (!g_storage_list_dir) return TERN_M5PAPER_BACKEND_NOT_FOUND;
    std::strncpy(g_storage_list_path, full_path, sizeof(g_storage_list_path) - 1);
    g_storage_list_path[sizeof(g_storage_list_path) - 1] = '\0';
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_storage_list_next(tern_m5paper_backend_storage_entry_t* out_entry) {
    if (!out_entry) return TERN_M5PAPER_BACKEND_IO_ERROR;
    if (!g_storage_list_dir) return TERN_M5PAPER_BACKEND_NOT_FOUND;
    while (true) {
        dirent* entry = readdir(g_storage_list_dir);
        if (!entry) return TERN_M5PAPER_BACKEND_NOT_FOUND;
        if (std::strcmp(entry->d_name, ".") == 0 || std::strcmp(entry->d_name, "..") == 0) continue;
        std::memset(out_entry, 0, sizeof(*out_entry));
        std::strncpy(out_entry->name, entry->d_name, sizeof(out_entry->name) - 1);
        char child_path[384];
        std::strncpy(child_path, g_storage_list_path, sizeof(child_path) - 1);
        size_t base_len = std::strlen(child_path);
        if (base_len + 1 + std::strlen(entry->d_name) + 1 > sizeof(child_path)) return TERN_M5PAPER_BACKEND_IO_ERROR;
        if (base_len > 1 || child_path[base_len - 1] != '/') std::strcat(child_path, "/");
        std::strcat(child_path, entry->d_name);
        struct stat st = {};
        if (stat(child_path, &st) == 0) {
            out_entry->is_dir = S_ISDIR(st.st_mode);
            out_entry->size = static_cast<uint32_t>(st.st_size);
        }
        return TERN_M5PAPER_BACKEND_OK;
    }
}

extern "C" void tern_m5paper_backend_storage_list_end(void) {
    if (g_storage_list_dir) {
        closedir(g_storage_list_dir);
        g_storage_list_dir = nullptr;
    }
    g_storage_list_path[0] = '\0';
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_storage_file_size(const char* path, uint32_t* out_size) {
    if (!path || !out_size) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = tern_m5paper_backend_storage_init();
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    struct stat st = {};
    auto stat_status = stat_storage_path(path, &st);
    if (stat_status != TERN_M5PAPER_BACKEND_OK) return stat_status;
    if (S_ISDIR(st.st_mode)) return TERN_M5PAPER_BACKEND_NOT_FOUND;
    *out_size = static_cast<uint32_t>(st.st_size);
    return TERN_M5PAPER_BACKEND_OK;
}

extern "C" tern_m5paper_backend_status_t tern_m5paper_backend_storage_read_chunk(const char* path, uint32_t offset, uint8_t* out_buf, uint32_t buf_len, uint32_t* out_read) {
    if (!path || !out_buf || !out_read) return TERN_M5PAPER_BACKEND_IO_ERROR;
    auto status = tern_m5paper_backend_storage_init();
    if (status != TERN_M5PAPER_BACKEND_OK) return status;
    char full_path[384];
    if (!normalize_storage_path(path, full_path, sizeof(full_path))) return TERN_M5PAPER_BACKEND_IO_ERROR;
    FILE* file = fopen(full_path, "rb");
    if (!file) return TERN_M5PAPER_BACKEND_NOT_FOUND;
    if (fseek(file, static_cast<long>(offset), SEEK_SET) != 0) {
        fclose(file);
        return TERN_M5PAPER_BACKEND_IO_ERROR;
    }
    *out_read = static_cast<uint32_t>(fread(out_buf, 1, buf_len, file));
    fclose(file);
    return TERN_M5PAPER_BACKEND_OK;
}
