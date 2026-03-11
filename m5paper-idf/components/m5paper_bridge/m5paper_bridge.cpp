#include "m5paper_bridge.h"

#include <cinttypes>

#include <Arduino.h>
#include <SPI.h>
#include <Wire.h>
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#include "M5EPD_Driver.h"
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

constexpr uint16_t kPanelWidth = 540;
constexpr uint16_t kPanelHeight = 960;
constexpr uint32_t kImageBufferAddr = 0x001236E0;
constexpr uint16_t kVcomMv = 2300;
constexpr const char* kLogTag = "m5paper_bridge";
constexpr uint16_t kTestRectWidth = 64;
constexpr uint16_t kTestRectHeight = 64;
constexpr size_t kTestRectBytes = (kTestRectWidth * kTestRectHeight) / 2;

bool g_arduino_ready = false;
bool g_board_ready = false;
bool g_epd_ready = false;
bool g_touch_ready = false;
M5EPD_Driver g_epd(VSPI);
GT911 g_touch;
uint8_t g_test_rect[kTestRectBytes];

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

}  // namespace

extern "C" void app_main(void) {
    auto board = tern_m5paper_board_init();
    ESP_LOGI(kLogTag, "board_init=%d", static_cast<int>(board));

    tern_m5paper_epd_info_t info{};
    auto epd = tern_m5paper_epd_init(&info);
    ESP_LOGI(
        kLogTag,
        "epd_init=%d panel=%ux%u img_buf=0x%08" PRIX32 " vcom=%umV",
        static_cast<int>(epd),
        info.panel_width,
        info.panel_height,
        info.image_buffer_addr,
        info.vcom_mv
    );
    auto clear = tern_m5paper_epd_clear(true);
    ESP_LOGI(kLogTag, "epd_clear=%d", static_cast<int>(clear));

    memset(g_test_rect, 0xFF, sizeof(g_test_rect));
    auto rect = tern_m5paper_epd_update_region(0, 0, kTestRectWidth, kTestRectHeight, g_test_rect, sizeof(g_test_rect));
    ESP_LOGI(
        kLogTag,
        "epd_test_rect=%d x=%u y=%u w=%u h=%u bytes=%u",
        static_cast<int>(rect),
        0u,
        0u,
        kTestRectWidth,
        kTestRectHeight,
        static_cast<unsigned>(sizeof(g_test_rect))
    );

    auto touch = tern_m5paper_touch_init();
    ESP_LOGI(kLogTag, "touch_init=%d", static_cast<int>(touch));
    tern_m5paper_touch_state_t touch_state{};
    auto touch_read = tern_m5paper_touch_read(&touch_state);
    ESP_LOGI(
        kLogTag,
        "touch_read=%d touched=%d x=%u y=%u count=%u",
        static_cast<int>(touch_read),
        touch_state.touched ? 1 : 0,
        touch_state.x,
        touch_state.y,
        touch_state.count
    );

    while (true) {
        vTaskDelay(pdMS_TO_TICKS(1000));
    }
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
