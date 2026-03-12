#include "m5paper_bridge.h"

#include <algorithm>
#include <array>
#include <cctype>
#include <cinttypes>
#include <cstdio>
#include <cstring>
#include <dirent.h>
#include <string>
#include <strings.h>
#include <sys/stat.h>
#include <vector>

#include <Arduino.h>
#include <SD.h>
#include <SPI.h>
#include <Wire.h>
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
constexpr const char* kLogTag = "m5paper_bridge";
constexpr uint32_t kBridgeTaskStackWords = 12 * 1024;
constexpr UBaseType_t kBridgeTaskPriority = 5;
constexpr size_t kInputQueueCapacity = 64;

bool g_arduino_ready = false;
bool g_board_ready = false;
bool g_epd_ready = false;
bool g_touch_ready = false;
bool g_rtc_ready = false;
bool g_storage_ready = false;
bool g_bridge_started = false;
M5EPD_Driver g_epd(VSPI);
BM8563 g_rtc;
GT911 g_touch;
DIR* g_storage_list_dir = nullptr;
char g_storage_list_path[384] = {};
std::array<tern_m5paper_input_event_t, kInputQueueCapacity> g_input_queue = {};
size_t g_input_queue_head = 0;
size_t g_input_queue_tail = 0;

constexpr const char* kStorageMountPoint = "/sd";
constexpr int kHeaderHeight = 64;
constexpr int kRowHeight = 44;
constexpr int kRowCount = 5;
constexpr int kListTop = 112;
constexpr int kFooterTop = kListTop + kRowCount * kRowHeight + 16;
constexpr int kFooterHeight = 88;
constexpr int kTextScale = 2;

enum class HomeCategory : uint8_t {
    Apps,
    Books,
    Images,
};

struct BootstrapEntry {
    bool is_dir;
    uint32_t size;
    char name[80];
};

std::vector<BootstrapEntry> g_bootstrap_entries = {};
std::vector<size_t> g_visible_entries = {};
size_t g_bootstrap_top_index = 0;
size_t g_selected_index = 0;
uint8_t g_last_rtc_second = 0xFF;
HomeCategory g_home_category = HomeCategory::Books;

bool normalize_storage_path(const char* path, char* out_path, size_t out_len) {
    if (out_path == nullptr || out_len == 0) {
        return false;
    }
    if (path == nullptr || path[0] == '\0' || std::strcmp(path, "/") == 0) {
        return std::snprintf(out_path, out_len, "%s", kStorageMountPoint) < static_cast<int>(out_len);
    }
    if (path[0] == '/') {
        return std::snprintf(out_path, out_len, "%s%s", kStorageMountPoint, path) < static_cast<int>(out_len);
    }
    return std::snprintf(out_path, out_len, "%s/%s", kStorageMountPoint, path) < static_cast<int>(out_len);
}

tern_m5paper_status_t stat_storage_path(const char* path, struct stat* st) {
    char full_path[384];
    if (!normalize_storage_path(path, full_path, sizeof(full_path))) {
        return TERN_M5PAPER_IO_ERROR;
    }
    if (stat(full_path, st) != 0) {
        return TERN_M5PAPER_NOT_FOUND;
    }
    return TERN_M5PAPER_OK;
}

tern_m5paper_status_t map_epd_status(m5epd_err_t status) {
    switch (status) {
        case M5EPD_OK:
            return TERN_M5PAPER_OK;
        case M5EPD_BUSYTIMEOUT:
            return TERN_M5PAPER_TIMEOUT;
        default:
            return TERN_M5PAPER_IO_ERROR;
    }
}

void ensure_arduino() {
    if (!g_arduino_ready) {
        initArduino();
        g_arduino_ready = true;
    }
}

void push_input_event(const tern_m5paper_input_event_t& event) {
    const size_t next_tail = (g_input_queue_tail + 1) % kInputQueueCapacity;
    if (next_tail == g_input_queue_head) {
        g_input_queue_head = (g_input_queue_head + 1) % kInputQueueCapacity;
    }
    g_input_queue[g_input_queue_tail] = event;
    g_input_queue_tail = next_tail;
}

void put_pixel(std::vector<uint8_t>& buf, int width, int x, int y, uint8_t color) {
    if (x < 0 || y < 0 || x >= width) return;
    const size_t byte_index = static_cast<size_t>(y) * static_cast<size_t>(width / 2) + static_cast<size_t>(x / 2);
    if ((x & 1) == 0) {
        buf[byte_index] = static_cast<uint8_t>((buf[byte_index] & 0x0F) | (color << 4));
    } else {
        buf[byte_index] = static_cast<uint8_t>((buf[byte_index] & 0xF0) | (color & 0x0F));
    }
}

void fill_rect(std::vector<uint8_t>& buf, int width, int height, int x, int y, int w, int h, uint8_t color) {
    const int x0 = std::max(0, x);
    const int y0 = std::max(0, y);
    const int x1 = std::min(width, x + w);
    const int y1 = std::min(height, y + h);
    for (int yy = y0; yy < y1; ++yy) {
        for (int xx = x0; xx < x1; ++xx) {
            put_pixel(buf, width, xx, yy, color);
        }
    }
}

const uint8_t* glyph_rows(char ch) {
    static const uint8_t SPACE[7] = {0, 0, 0, 0, 0, 0, 0};
    static const uint8_t DASH[7] = {0, 0, 0, 0x1F, 0, 0, 0};
    static const uint8_t DOT[7] = {0, 0, 0, 0, 0, 0x0C, 0x0C};
    static const uint8_t COLON[7] = {0, 0x0C, 0x0C, 0, 0x0C, 0x0C, 0};
    static const uint8_t SLASH[7] = {0x01, 0x02, 0x04, 0x08, 0x10, 0, 0};
    static const uint8_t UNDERSCORE[7] = {0, 0, 0, 0, 0, 0, 0x1F};
    static const uint8_t DIGITS[10][7] = {
        {0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E},
        {0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E},
        {0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F},
        {0x1F, 0x02, 0x04, 0x06, 0x01, 0x11, 0x0E},
        {0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02},
        {0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E},
        {0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E},
        {0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08},
        {0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E},
        {0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C},
    };
    static const uint8_t LETTERS[26][7] = {
        {0x0E,0x11,0x11,0x1F,0x11,0x11,0x11}, {0x1E,0x11,0x11,0x1E,0x11,0x11,0x1E},
        {0x0E,0x11,0x10,0x10,0x10,0x11,0x0E}, {0x1C,0x12,0x11,0x11,0x11,0x12,0x1C},
        {0x1F,0x10,0x10,0x1E,0x10,0x10,0x1F}, {0x1F,0x10,0x10,0x1E,0x10,0x10,0x10},
        {0x0E,0x11,0x10,0x17,0x11,0x11,0x0E}, {0x11,0x11,0x11,0x1F,0x11,0x11,0x11},
        {0x0E,0x04,0x04,0x04,0x04,0x04,0x0E}, {0x01,0x01,0x01,0x01,0x11,0x11,0x0E},
        {0x11,0x12,0x14,0x18,0x14,0x12,0x11}, {0x10,0x10,0x10,0x10,0x10,0x10,0x1F},
        {0x11,0x1B,0x15,0x15,0x11,0x11,0x11}, {0x11,0x19,0x15,0x13,0x11,0x11,0x11},
        {0x0E,0x11,0x11,0x11,0x11,0x11,0x0E}, {0x1E,0x11,0x11,0x1E,0x10,0x10,0x10},
        {0x0E,0x11,0x11,0x11,0x15,0x12,0x0D}, {0x1E,0x11,0x11,0x1E,0x14,0x12,0x11},
        {0x0F,0x10,0x10,0x0E,0x01,0x01,0x1E}, {0x1F,0x04,0x04,0x04,0x04,0x04,0x04},
        {0x11,0x11,0x11,0x11,0x11,0x11,0x0E}, {0x11,0x11,0x11,0x11,0x11,0x0A,0x04},
        {0x11,0x11,0x11,0x15,0x15,0x15,0x0A}, {0x11,0x11,0x0A,0x04,0x0A,0x11,0x11},
        {0x11,0x11,0x0A,0x04,0x04,0x04,0x04}, {0x1F,0x01,0x02,0x04,0x08,0x10,0x1F},
    };

    if (ch >= 'a' && ch <= 'z') ch = static_cast<char>(std::toupper(static_cast<unsigned char>(ch)));
    if (ch >= 'A' && ch <= 'Z') return LETTERS[ch - 'A'];
    if (ch >= '0' && ch <= '9') return DIGITS[ch - '0'];
    switch (ch) {
        case '-': return DASH;
        case '.': return DOT;
        case ':': return COLON;
        case '/': return SLASH;
        case '_': return UNDERSCORE;
        case ' ': return SPACE;
        default: return SPACE;
    }
}

void draw_char(std::vector<uint8_t>& buf, int width, int height, int x, int y, char ch, uint8_t color, int scale) {
    const uint8_t* rows = glyph_rows(ch);
    for (int row = 0; row < 7; ++row) {
        for (int col = 0; col < 5; ++col) {
            if ((rows[row] >> (4 - col)) & 0x01) {
                fill_rect(buf, width, height, x + col * scale, y + row * scale, scale, scale, color);
            }
        }
    }
}

void draw_text(std::vector<uint8_t>& buf, int width, int height, int x, int y, const char* text, uint8_t color, int scale) {
    int cursor = x;
    for (const char* p = text; *p != '\0'; ++p) {
        draw_char(buf, width, height, cursor, y, *p, color, scale);
        cursor += 6 * scale;
    }
}

tern_m5paper_status_t update_region_buffer(int x, int y, int width, int height, const std::vector<uint8_t>& buf) {
    return tern_m5paper_epd_update_region(
        static_cast<uint16_t>(x),
        static_cast<uint16_t>(y),
        static_cast<uint16_t>(width),
        static_cast<uint16_t>(height),
        buf.data(),
        static_cast<uint32_t>(buf.size())
    );
}

bool entry_matches_category(const BootstrapEntry& entry, HomeCategory category) {
    if (entry.is_dir) {
        return false;
    }
    const char* ext = std::strrchr(entry.name, '.');
    if (ext == nullptr) {
        return false;
    }
    if (strcasecmp(ext, ".prc") == 0 || strcasecmp(ext, ".tdb") == 0) {
        return category == HomeCategory::Apps;
    }
    if (strcasecmp(ext, ".trbk") == 0 || strcasecmp(ext, ".tbk") == 0 ||
        strcasecmp(ext, ".epub") == 0 || strcasecmp(ext, ".epb") == 0) {
        return category == HomeCategory::Books;
    }
    if (strcasecmp(ext, ".png") == 0 || strcasecmp(ext, ".jpg") == 0 ||
        strcasecmp(ext, ".jpeg") == 0 || strcasecmp(ext, ".tri") == 0) {
        return category == HomeCategory::Images;
    }
    return false;
}

const char* category_label(HomeCategory category) {
    switch (category) {
        case HomeCategory::Apps: return "APPS";
        case HomeCategory::Books: return "BOOKS";
        case HomeCategory::Images: return "IMAGES";
    }
    return "BOOKS";
}

void rebuild_visible_entries() {
    g_visible_entries.clear();
    for (size_t i = 0; i < g_bootstrap_entries.size(); ++i) {
        if (entry_matches_category(g_bootstrap_entries[i], g_home_category)) {
            g_visible_entries.push_back(i);
        }
    }
    if (g_visible_entries.empty()) {
        g_selected_index = 0;
        g_bootstrap_top_index = 0;
        return;
    }
    if (g_selected_index >= g_visible_entries.size()) {
        g_selected_index = 0;
    }
    if (g_bootstrap_top_index > g_selected_index) {
        g_bootstrap_top_index = g_selected_index;
    }
}

const BootstrapEntry* selected_entry() {
    if (g_selected_index >= g_visible_entries.size()) {
        return nullptr;
    }
    return &g_bootstrap_entries[g_visible_entries[g_selected_index]];
}

void load_bootstrap_entries() {
    g_bootstrap_entries.clear();
    if (tern_m5paper_storage_list_begin("/") != TERN_M5PAPER_OK) {
        return;
    }
    while (true) {
        tern_m5paper_storage_entry_t entry = {};
        if (tern_m5paper_storage_list_next(&entry) != TERN_M5PAPER_OK) {
            break;
        }
        BootstrapEntry dst = {};
        dst.is_dir = entry.is_dir;
        dst.size = entry.size;
        std::strncpy(dst.name, entry.name, sizeof(dst.name) - 1);
        dst.name[sizeof(dst.name) - 1] = '\0';
        g_bootstrap_entries.push_back(dst);
    }
    tern_m5paper_storage_list_end();
    rebuild_visible_entries();
}

void render_header() {
    std::vector<uint8_t> buf(static_cast<size_t>(kPanelWidth * kHeaderHeight) / 2, 0x00);
    fill_rect(buf, kPanelWidth, kHeaderHeight, 0, 0, kPanelWidth, kHeaderHeight, 1);

    const int tab_x = 10;
    const int tab_y = 8;
    const int tab_w = 108;
    const int tab_h = 32;
    fill_rect(buf, kPanelWidth, kHeaderHeight, tab_x, tab_y, tab_w, tab_h, 15);
    fill_rect(buf, kPanelWidth, kHeaderHeight, 0, tab_y + tab_h + 4, kPanelWidth, 2, 8);
    draw_text(buf, kPanelWidth, kHeaderHeight, tab_x + 12, tab_y + 6, "HOME", 1, 2);
    draw_text(buf, kPanelWidth, kHeaderHeight, 152, 12, category_label(g_home_category), 12, 2);
    draw_text(buf, kPanelWidth, kHeaderHeight, 152, 34, "/SD", 8, 1);

    const struct {
        HomeCategory category;
        int x;
        const char* label;
    } categories[] = {
        {HomeCategory::Apps, 260, "APPS"},
        {HomeCategory::Books, 344, "BOOKS"},
        {HomeCategory::Images, 440, "IMAGES"},
    };
    for (const auto& cat : categories) {
        const bool selected = cat.category == g_home_category;
        fill_rect(buf, kPanelWidth, kHeaderHeight, cat.x, 10, 72, 24, selected ? 12 : 3);
        draw_text(buf, kPanelWidth, kHeaderHeight, cat.x + 8, 16, cat.label, selected ? 0 : 10, 1);
    }

    tern_m5paper_rtc_datetime_t rtc = {};
    if (tern_m5paper_rtc_read(&rtc) == TERN_M5PAPER_OK) {
        char time_buf[16];
        std::snprintf(time_buf, sizeof(time_buf), "%02u:%02u", rtc.hour, rtc.minute);
        draw_text(buf, kPanelWidth, kHeaderHeight, 432, 16, time_buf, 12, 2);
        g_last_rtc_second = rtc.second;
    }
    update_region_buffer(0, 0, kPanelWidth, kHeaderHeight, buf);
}

void render_footer() {
    std::vector<uint8_t> buf(static_cast<size_t>(kPanelWidth * kFooterHeight) / 2, 0x00);
    fill_rect(buf, kPanelWidth, kFooterHeight, 0, 0, kPanelWidth, kFooterHeight, 1);
    fill_rect(buf, kPanelWidth, kFooterHeight, 0, 0, kPanelWidth, 2, 6);

    char summary[96];
    if (const auto* entry = selected_entry()) {
        if (entry->is_dir) {
            std::snprintf(summary, sizeof(summary), "DIR  %s", entry->name);
        } else {
            std::snprintf(summary, sizeof(summary), "FILE %s  %luK", entry->name, static_cast<unsigned long>((entry->size + 1023) / 1024));
        }
    } else {
        std::snprintf(summary, sizeof(summary), "NO SELECTION");
    }

    draw_text(buf, kPanelWidth, kFooterHeight, 12, 14, summary, 12, 1);
    char nav_buf[48];
    std::snprintf(
        nav_buf,
        sizeof(nav_buf),
        "%lu/%lu",
        static_cast<unsigned long>(g_visible_entries.empty() ? 0 : g_selected_index + 1),
        static_cast<unsigned long>(g_visible_entries.size())
    );
    draw_text(buf, kPanelWidth, kFooterHeight, 12, 48, "UP/DOWN MOVE", 10, 1);
    draw_text(buf, kPanelWidth, kFooterHeight, 152, 48, nav_buf, 12, 1);
    draw_text(buf, kPanelWidth, kFooterHeight, 224, 48, "TOUCH SELECT", 10, 1);
    draw_text(buf, kPanelWidth, kFooterHeight, 424, 48, "POWER OPEN", 10, 1);
    update_region_buffer(0, kFooterTop, kPanelWidth, kFooterHeight, buf);
}

void render_row(size_t visible_index) {
    const int y = kListTop + static_cast<int>(visible_index) * kRowHeight;
    std::vector<uint8_t> buf(static_cast<size_t>(kPanelWidth * kRowHeight) / 2, 0x00);
    const size_t visible_entry_index = g_bootstrap_top_index + visible_index;
    const bool selected = visible_entry_index == g_selected_index;
    const uint8_t bg = selected ? 12 : 1;
    const uint8_t fg = selected ? 0 : 14;
    fill_rect(buf, kPanelWidth, kRowHeight, 0, 0, kPanelWidth, kRowHeight, bg);
    fill_rect(buf, kPanelWidth, kRowHeight, 0, kRowHeight - 1, kPanelWidth, 1, 5);
    if (visible_entry_index < g_visible_entries.size()) {
        const auto& entry = g_bootstrap_entries[g_visible_entries[visible_entry_index]];
        fill_rect(buf, kPanelWidth, kRowHeight, 12, 10, 20, 20, entry.is_dir ? 6 : 10);
        draw_text(buf, kPanelWidth, kRowHeight, 44, 10, entry.name, fg, kTextScale);
    } else {
        draw_text(buf, kPanelWidth, kRowHeight, 44, 10, "-", fg, kTextScale);
    }
    update_region_buffer(0, y, kPanelWidth, kRowHeight, buf);
}

void render_bootstrap_screen() {
    render_header();
    for (size_t i = 0; i < kRowCount; ++i) {
        render_row(i);
    }
    render_footer();
}

void ensure_selection_visible() {
    if (g_selected_index < g_bootstrap_top_index) {
        g_bootstrap_top_index = g_selected_index;
    } else if (g_selected_index >= g_bootstrap_top_index + kRowCount) {
        g_bootstrap_top_index = g_selected_index + 1 - kRowCount;
    }
}

void rerender_visible_rows() {
    for (size_t i = 0; i < kRowCount; ++i) {
        render_row(i);
    }
    render_footer();
}

void update_selection_from_touch(uint16_t x, uint16_t y) {
    if (y < kHeaderHeight) {
        struct CategoryHit {
            HomeCategory category;
            uint16_t x0;
            uint16_t x1;
        };
        constexpr CategoryHit hits[] = {
            {HomeCategory::Apps, 260, 332},
            {HomeCategory::Books, 344, 416},
            {HomeCategory::Images, 440, 512},
        };
        for (const auto& hit : hits) {
            if (x >= hit.x0 && x < hit.x1 && g_home_category != hit.category) {
                g_home_category = hit.category;
                g_selected_index = 0;
                g_bootstrap_top_index = 0;
                rebuild_visible_entries();
                render_bootstrap_screen();
                return;
            }
        }
    }

    if (g_visible_entries.empty()) {
        return;
    }
    if (y < kListTop || y >= kListTop + static_cast<uint16_t>(kRowCount * kRowHeight)) {
        return;
    }
    const size_t tapped = g_bootstrap_top_index + static_cast<size_t>((y - kListTop) / kRowHeight);
    if (tapped >= g_visible_entries.size() || tapped == g_selected_index) {
        return;
    }
    g_selected_index = tapped;
    ensure_selection_visible();
    rerender_visible_rows();
}

void bridge_task(void*) {
    tern_m5paper_button_state_t last_buttons = {};
    tern_m5paper_touch_state_t last_touch = {};
    while (true) {
        tern_m5paper_button_state_t buttons = {};
        if (tern_m5paper_buttons_read(&buttons) == TERN_M5PAPER_OK) {
            if (buttons.up_pressed != last_buttons.up_pressed) {
                push_input_event({
                    static_cast<uint8_t>(buttons.up_pressed ? TERN_M5PAPER_INPUT_BUTTON_DOWN
                                                           : TERN_M5PAPER_INPUT_BUTTON_UP),
                    TERN_M5PAPER_BUTTON_UP,
                    0,
                    0,
                    0,
                });
                if (buttons.up_pressed && !g_visible_entries.empty()) {
                    if (g_selected_index == 0) {
                        g_selected_index = g_visible_entries.size() - 1;
                    } else {
                        g_selected_index -= 1;
                    }
                    ensure_selection_visible();
                    rerender_visible_rows();
                }
            }
            if (buttons.power_pressed != last_buttons.power_pressed) {
                push_input_event({
                    static_cast<uint8_t>(buttons.power_pressed ? TERN_M5PAPER_INPUT_BUTTON_DOWN
                                                               : TERN_M5PAPER_INPUT_BUTTON_UP),
                    TERN_M5PAPER_BUTTON_POWER,
                    0,
                    0,
                    0,
                });
                if (buttons.power_pressed) {
                    const auto* entry = selected_entry();
                    if (entry != nullptr) {
                        ESP_LOGI(
                            kLogTag,
                            "bootstrap_open category=%s type=%s name=%s size=%" PRIu32,
                            category_label(g_home_category),
                            entry->is_dir ? "dir" : "file",
                            entry->name,
                            entry->size
                        );
                    }
                }
            }
            if (buttons.down_pressed != last_buttons.down_pressed) {
                push_input_event({
                    static_cast<uint8_t>(buttons.down_pressed ? TERN_M5PAPER_INPUT_BUTTON_DOWN
                                                              : TERN_M5PAPER_INPUT_BUTTON_UP),
                    TERN_M5PAPER_BUTTON_DOWN,
                    0,
                    0,
                    0,
                });
                if (buttons.down_pressed && !g_visible_entries.empty()) {
                    g_selected_index = (g_selected_index + 1) % g_visible_entries.size();
                    ensure_selection_visible();
                    rerender_visible_rows();
                }
            }
            last_buttons = buttons;
        }

        tern_m5paper_touch_state_t touch = {};
        if (tern_m5paper_touch_read(&touch) == TERN_M5PAPER_OK) {
            if (touch.touched != last_touch.touched) {
                push_input_event({
                    static_cast<uint8_t>(touch.touched ? TERN_M5PAPER_INPUT_TOUCH_DOWN
                                                       : TERN_M5PAPER_INPUT_TOUCH_UP),
                    0,
                    touch.x,
                    touch.y,
                    touch.count,
                });
                if (touch.touched) {
                    update_selection_from_touch(touch.x, touch.y);
                }
                last_touch = touch;
            } else if (touch.touched &&
                       (touch.x != last_touch.x || touch.y != last_touch.y || touch.count != last_touch.count)) {
                push_input_event({
                    TERN_M5PAPER_INPUT_TOUCH_MOVE,
                    0,
                    touch.x,
                    touch.y,
                    touch.count,
                });
                update_selection_from_touch(touch.x, touch.y);
                last_touch = touch;
            }
        }

        tern_m5paper_rtc_datetime_t rtc = {};
        if (tern_m5paper_rtc_read(&rtc) == TERN_M5PAPER_OK && rtc.second != g_last_rtc_second) {
            render_header();
        }


        vTaskDelay(pdMS_TO_TICKS(20));
    }
}

}  // namespace

extern "C" tern_m5paper_status_t tern_m5paper_bridge_start(void) {
    if (g_bridge_started) {
        return TERN_M5PAPER_OK;
    }

    tern_m5paper_epd_info_t info = {};

    auto board_status = tern_m5paper_board_init();
    ESP_LOGI(kLogTag, "board_init=%d", static_cast<int>(board_status));
    if (board_status != TERN_M5PAPER_OK) {
        return board_status;
    }

    auto epd_status = tern_m5paper_epd_init(&info);
    ESP_LOGI(
        kLogTag,
        "epd_init=%d panel=%" PRIu16 "x%" PRIu16 " img_buf=0x%08" PRIX32 " vcom=%" PRIu16 "mV",
        static_cast<int>(epd_status),
        info.panel_width,
        info.panel_height,
        info.image_buffer_addr,
        info.vcom_mv
    );
    if (epd_status != TERN_M5PAPER_OK) {
        return epd_status;
    }

    auto clear_status = tern_m5paper_epd_clear(true);
    ESP_LOGI(kLogTag, "epd_clear=%d", static_cast<int>(clear_status));
    if (clear_status != TERN_M5PAPER_OK) {
        return clear_status;
    }

    auto touch_status = tern_m5paper_touch_init();
    ESP_LOGI(kLogTag, "touch_init=%d", static_cast<int>(touch_status));
    if (touch_status != TERN_M5PAPER_OK) {
        return touch_status;
    }

    auto rtc_init_status = tern_m5paper_rtc_init();
    if (rtc_init_status != TERN_M5PAPER_OK) {
        return rtc_init_status;
    }

    auto storage_status = tern_m5paper_storage_init();
    ESP_LOGI(kLogTag, "storage_init=%d", static_cast<int>(storage_status));
    if (storage_status != TERN_M5PAPER_OK) {
        return storage_status;
    }

    if (tern_m5paper_storage_list_begin("/") == TERN_M5PAPER_OK) {
        ESP_LOGI(kLogTag, "sd_root_list begin");
        uint8_t count = 0;
        while (count < 32) {
            tern_m5paper_storage_entry_t entry = {};
            auto next_status = tern_m5paper_storage_list_next(&entry);
            if (next_status != TERN_M5PAPER_OK) {
                break;
            }
            ESP_LOGI(
                kLogTag,
                "sd_entry type=%s name=%s size=%u",
                entry.is_dir ? "dir" : "file",
                entry.name,
                static_cast<unsigned>(entry.size)
            );
            ++count;
        }
        tern_m5paper_storage_list_end();
        ESP_LOGI(kLogTag, "sd_root_list end count=%u", count);
    }

    load_bootstrap_entries();
    render_bootstrap_screen();

    BaseType_t ok = xTaskCreatePinnedToCore(
        bridge_task,
        "m5paper_bridge",
        kBridgeTaskStackWords,
        nullptr,
        kBridgeTaskPriority,
        nullptr,
        1
    );
    if (ok != pdPASS) {
        ESP_LOGE(kLogTag, "failed to create bridge task");
        return TERN_M5PAPER_IO_ERROR;
    }

    g_bridge_started = true;
    return TERN_M5PAPER_OK;
}

extern "C" void app_main(void) {
    auto status = tern_m5paper_bridge_start();
    ESP_LOGI(kLogTag, "app_main bridge_start=%d", static_cast<int>(status));
}

extern "C" tern_m5paper_status_t tern_m5paper_board_init(void) {
    ensure_arduino();

    if (g_board_ready) {
        return TERN_M5PAPER_OK;
    }

    pinMode(kMainPowerPin, OUTPUT);
    pinMode(kExtPowerPin, OUTPUT);
    pinMode(kEpdPowerPin, OUTPUT);
    pinMode(kTouchIntPin, INPUT);
    pinMode(kButtonUpPin, INPUT_PULLUP);
    pinMode(kButtonPowerPin, INPUT_PULLUP);
    pinMode(kButtonDownPin, INPUT_PULLUP);

    digitalWrite(kMainPowerPin, HIGH);
    delay(100);
    digitalWrite(kExtPowerPin, HIGH);
    digitalWrite(kEpdPowerPin, HIGH);
    delay(1000);

    g_board_ready = true;
    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_epd_init(tern_m5paper_epd_info_t* out_info) {
    auto board_status = tern_m5paper_board_init();
    if (board_status != TERN_M5PAPER_OK) {
        return board_status;
    }

    if (!g_epd_ready) {
        auto status = g_epd.begin(
            kEpdSckPin,
            kEpdMosiPin,
            kEpdMisoPin,
            kEpdCsPin,
            kEpdBusyPin
        );
        if (status != M5EPD_OK) {
            return map_epd_status(status);
        }
        g_epd.SetRotation(M5EPD_Driver::ROTATE_90);
        g_epd_ready = true;
    }

    if (out_info != nullptr) {
        out_info->panel_width = kPanelWidth;
        out_info->panel_height = kPanelHeight;
        out_info->image_buffer_addr = kImageBufferAddr;
        out_info->vcom_mv = kVcomMv;
    }

    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_epd_clear(bool init) {
    auto status = tern_m5paper_epd_init(nullptr);
    if (status != TERN_M5PAPER_OK) {
        return status;
    }
    return map_epd_status(g_epd.Clear(init));
}

extern "C" tern_m5paper_status_t tern_m5paper_epd_update_region(
    uint16_t x,
    uint16_t y,
    uint16_t width,
    uint16_t height,
    const uint8_t* data,
    uint32_t data_len
) {
    auto status = tern_m5paper_epd_init(nullptr);
    if (status != TERN_M5PAPER_OK) {
        return status;
    }
    if (data == nullptr || data_len == 0) {
        return TERN_M5PAPER_IO_ERROR;
    }

    const uint32_t expected_bytes = (static_cast<uint32_t>(width) * static_cast<uint32_t>(height)) / 2;
    if (data_len < expected_bytes) {
        return TERN_M5PAPER_IO_ERROR;
    }

    auto err = g_epd.WritePartGram4bpp(x, y, width, height, data);
    if (err != M5EPD_OK) {
        return map_epd_status(err);
    }
    err = g_epd.UpdateArea(x, y, width, height, UPDATE_MODE_GC16);
    return map_epd_status(err);
}

extern "C" tern_m5paper_status_t tern_m5paper_touch_init(void) {
    auto board_status = tern_m5paper_board_init();
    if (board_status != TERN_M5PAPER_OK) {
        return board_status;
    }

    if (!g_touch_ready) {
        if (g_touch.begin(kTouchSdaPin, kTouchSclPin, kTouchIntPin) != ESP_OK) {
            return TERN_M5PAPER_IO_ERROR;
        }
        g_touch.SetRotation(GT911::ROTATE_90);
        g_touch_ready = true;
    }

    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_touch_read(tern_m5paper_touch_state_t* out_state) {
    if (out_state == nullptr) {
        return TERN_M5PAPER_IO_ERROR;
    }

    auto status = tern_m5paper_touch_init();
    if (status != TERN_M5PAPER_OK) {
        return status;
    }

    if (g_touch.available()) {
        g_touch.update();
    }

    out_state->count = g_touch.getFingerNum();
    out_state->touched = out_state->count > 0;
    if (out_state->touched) {
        out_state->x = g_touch.readFingerX(0);
        out_state->y = g_touch.readFingerY(0);
    } else {
        out_state->x = 0;
        out_state->y = 0;
    }

    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_buttons_read(tern_m5paper_button_state_t* out_state) {
    if (out_state == nullptr) {
        return TERN_M5PAPER_IO_ERROR;
    }

    auto status = tern_m5paper_board_init();
    if (status != TERN_M5PAPER_OK) {
        return status;
    }

    out_state->up_pressed = digitalRead(kButtonUpPin) == LOW;
    out_state->power_pressed = digitalRead(kButtonPowerPin) == LOW;
    out_state->down_pressed = digitalRead(kButtonDownPin) == LOW;
    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_input_next(tern_m5paper_input_event_t* out_event) {
    if (out_event == nullptr) {
        return TERN_M5PAPER_IO_ERROR;
    }
    if (g_input_queue_head == g_input_queue_tail) {
        return TERN_M5PAPER_NOT_FOUND;
    }
    *out_event = g_input_queue[g_input_queue_head];
    g_input_queue_head = (g_input_queue_head + 1) % kInputQueueCapacity;
    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_rtc_init(void) {
    auto board_status = tern_m5paper_board_init();
    if (board_status != TERN_M5PAPER_OK) {
        return board_status;
    }

    if (!g_rtc_ready) {
        g_rtc.begin();
        g_rtc_ready = true;
    }

    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_rtc_read(tern_m5paper_rtc_datetime_t* out_datetime) {
    if (out_datetime == nullptr) {
        return TERN_M5PAPER_IO_ERROR;
    }

    auto status = tern_m5paper_rtc_init();
    if (status != TERN_M5PAPER_OK) {
        return status;
    }

    rtc_date_t date;
    rtc_time_t time;
    g_rtc.getDate(&date);
    g_rtc.getTime(&time);

    out_datetime->year = static_cast<uint16_t>(date.year);
    out_datetime->month = static_cast<uint8_t>(date.mon);
    out_datetime->day = static_cast<uint8_t>(date.day);
    out_datetime->week = static_cast<uint8_t>(date.week);
    out_datetime->hour = static_cast<uint8_t>(time.hour);
    out_datetime->minute = static_cast<uint8_t>(time.min);
    out_datetime->second = static_cast<uint8_t>(time.sec);
    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_rtc_set(const tern_m5paper_rtc_datetime_t* datetime) {
    if (datetime == nullptr) {
        return TERN_M5PAPER_IO_ERROR;
    }

    auto status = tern_m5paper_rtc_init();
    if (status != TERN_M5PAPER_OK) {
        return status;
    }

    rtc_date_t date;
    date.year = static_cast<int16_t>(datetime->year);
    date.mon = static_cast<int8_t>(datetime->month);
    date.day = static_cast<int8_t>(datetime->day);
    date.week = static_cast<int8_t>(datetime->week);

    rtc_time_t time;
    time.hour = static_cast<int8_t>(datetime->hour);
    time.min = static_cast<int8_t>(datetime->minute);
    time.sec = static_cast<int8_t>(datetime->second);

    if (!g_rtc.setDate(&date) || !g_rtc.setTime(&time)) {
        return TERN_M5PAPER_IO_ERROR;
    }

    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_storage_init(void) {
    auto board_status = tern_m5paper_board_init();
    if (board_status != TERN_M5PAPER_OK) {
        return board_status;
    }

    auto epd_status = tern_m5paper_epd_init(nullptr);
    if (epd_status != TERN_M5PAPER_OK) {
        return epd_status;
    }

    if (!g_storage_ready) {
        if (!SD.begin(4, *g_epd.GetSPI(), 20000000, kStorageMountPoint, 8, false)) {
            return TERN_M5PAPER_IO_ERROR;
        }
        g_storage_ready = true;
    }

    return TERN_M5PAPER_OK;
}

extern "C" bool tern_m5paper_storage_exists(const char* path) {
    if (path == nullptr || tern_m5paper_storage_init() != TERN_M5PAPER_OK) {
        return false;
    }
    struct stat st = {};
    return stat_storage_path(path, &st) == TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_storage_list_begin(const char* path) {
    if (path == nullptr) {
        return TERN_M5PAPER_IO_ERROR;
    }
    auto status = tern_m5paper_storage_init();
    if (status != TERN_M5PAPER_OK) {
        return status;
    }

    if (g_storage_list_dir != nullptr) {
        closedir(g_storage_list_dir);
        g_storage_list_dir = nullptr;
    }

    char full_path[384];
    if (!normalize_storage_path(path, full_path, sizeof(full_path))) {
        return TERN_M5PAPER_IO_ERROR;
    }

    g_storage_list_dir = opendir(full_path);
    if (g_storage_list_dir == nullptr) {
        return TERN_M5PAPER_NOT_FOUND;
    }
    std::strncpy(g_storage_list_path, full_path, sizeof(g_storage_list_path) - 1);
    g_storage_list_path[sizeof(g_storage_list_path) - 1] = '\0';
    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_storage_list_next(tern_m5paper_storage_entry_t* out_entry) {
    if (out_entry == nullptr) {
        return TERN_M5PAPER_IO_ERROR;
    }
    if (g_storage_list_dir == nullptr) {
        return TERN_M5PAPER_NOT_FOUND;
    }

    while (true) {
        struct dirent* entry = readdir(g_storage_list_dir);
        if (entry == nullptr) {
            return TERN_M5PAPER_NOT_FOUND;
        }
        if (std::strcmp(entry->d_name, ".") == 0 || std::strcmp(entry->d_name, "..") == 0) {
            continue;
        }

        memset(out_entry, 0, sizeof(*out_entry));
        std::strncpy(out_entry->name, entry->d_name, sizeof(out_entry->name) - 1);
        out_entry->name[sizeof(out_entry->name) - 1] = '\0';

        char child_path[384];
        std::strncpy(child_path, g_storage_list_path, sizeof(child_path) - 1);
        child_path[sizeof(child_path) - 1] = '\0';
        size_t base_len = std::strlen(child_path);
        if (base_len + 1 + std::strlen(entry->d_name) + 1 > sizeof(child_path)) {
            return TERN_M5PAPER_IO_ERROR;
        }
        if (base_len > 1 || child_path[base_len - 1] != '/') {
            std::strcat(child_path, "/");
        }
        std::strcat(child_path, entry->d_name);

        struct stat st = {};
        if (stat(child_path, &st) != 0) {
            out_entry->is_dir = false;
            out_entry->size = 0;
        } else {
            out_entry->is_dir = S_ISDIR(st.st_mode);
            out_entry->size = static_cast<uint32_t>(st.st_size);
        }
        return TERN_M5PAPER_OK;
    }
}

extern "C" void tern_m5paper_storage_list_end(void) {
    if (g_storage_list_dir != nullptr) {
        closedir(g_storage_list_dir);
        g_storage_list_dir = nullptr;
    }
    g_storage_list_path[0] = '\0';
}

extern "C" tern_m5paper_status_t tern_m5paper_storage_file_size(const char* path, uint32_t* out_size) {
    if (path == nullptr || out_size == nullptr) {
        return TERN_M5PAPER_IO_ERROR;
    }
    auto status = tern_m5paper_storage_init();
    if (status != TERN_M5PAPER_OK) {
        return status;
    }

    struct stat st = {};
    auto stat_status = stat_storage_path(path, &st);
    if (stat_status != TERN_M5PAPER_OK) {
        return stat_status;
    }
    if (S_ISDIR(st.st_mode)) {
        return TERN_M5PAPER_NOT_FOUND;
    }
    *out_size = static_cast<uint32_t>(st.st_size);
    return TERN_M5PAPER_OK;
}

extern "C" tern_m5paper_status_t tern_m5paper_storage_read_chunk(const char* path,
                                                                  uint32_t offset,
                                                                  uint8_t* out_buf,
                                                                  uint32_t buf_len,
                                                                  uint32_t* out_read) {
    if (path == nullptr || out_buf == nullptr || out_read == nullptr) {
        return TERN_M5PAPER_IO_ERROR;
    }
    auto status = tern_m5paper_storage_init();
    if (status != TERN_M5PAPER_OK) {
        return status;
    }

    char full_path[384];
    if (!normalize_storage_path(path, full_path, sizeof(full_path))) {
        return TERN_M5PAPER_IO_ERROR;
    }

    FILE* file = fopen(full_path, "rb");
    if (file == nullptr) {
        return TERN_M5PAPER_NOT_FOUND;
    }
    if (fseek(file, static_cast<long>(offset), SEEK_SET) != 0) {
        fclose(file);
        return TERN_M5PAPER_IO_ERROR;
    }
    *out_read = static_cast<uint32_t>(fread(out_buf, 1, buf_len, file));
    fclose(file);
    return TERN_M5PAPER_OK;
}
