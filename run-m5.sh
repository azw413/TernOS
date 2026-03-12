#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/m5paper"

source /Users/andrew/export-esp.sh

rm -rf ../target/xtensa-esp32-espidf ../target/release/build/esp-idf-sys-*

cargo +esp build --release --features cshim

elf_path="$(ls -td ../target/xtensa-esp32-espidf/release/build/esp-idf-sys-*/out/build/libespidf.elf | head -n1)"

if [[ -z "${elf_path}" || ! -f "${elf_path}" ]]; then
  echo "Failed to locate libespidf.elf after build" >&2
  exit 1
fi

espflash flash --monitor --chip esp32 --port /dev/cu.usbserial-0214212B "${elf_path}"
