#!/bin/bash
set -euo pipefail

source /Users/andrew/export-esp.sh
cd "$(dirname "$0")/m5paper"

cargo +esp espflash save-image --release --chip=esp32 --target=xtensa-esp32-none-elf firmware.bin
if [[ $(stat -f%z firmware.bin) -gt 6553600 ]]; then
    echo -e "\033[0;31m[ERROR] Firmware size exceeds OFW partition limit!"
    exit 1
fi
cargo +esp espflash write-bin 0x10000 firmware.bin
echo "Firmware written. Starting monitor; press CTRL+R or the device reset button to boot."
cargo +esp espflash monitor --chip esp32 --port /dev/cu.usbserial-0214212B
