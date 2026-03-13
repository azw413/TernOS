#include <stdint.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

void tern_runtime_main(void);

static void tern_runtime_task(void* arg) {
    (void)arg;
    tern_runtime_main();
    vTaskDelete(NULL);
}

void app_main(void) {
    xTaskCreatePinnedToCore(
        tern_runtime_task,
        "tern_runtime",
        32768,
        NULL,
        5,
        NULL,
        1
    );
}
