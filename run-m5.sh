#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/m5paper"

source /Users/andrew/export-esp.sh
export IDF_TOOLS_PATH="$(pwd)/../.embuild/espressif"
export ESP_IDF_COMPONENT_MANAGER=false

rm -rf ../target/xtensa-esp32-espidf ../target/release/build/esp-idf-sys-*

build_log="../target/run-m5-build.log"
mkdir -p ../target
if ! cargo +esp build --release --features cshim >"${build_log}" 2>&1; then
  echo "M5Paper build failed. Last 200 lines from ${build_log}:" >&2
  tail -n 200 "${build_log}" >&2 || true
  exit 1
fi

elf_path="$(ls -td ../target/xtensa-esp32-espidf/release/build/esp-idf-sys-*/out/build/libespidf.elf | head -n1)"

if [[ -z "${elf_path}" || ! -f "${elf_path}" ]]; then
  echo "Failed to locate libespidf.elf after build" >&2
  exit 1
fi

espflash flash --monitor --chip esp32 --port /dev/cu.usbserial-0214212B "${elf_path}"
