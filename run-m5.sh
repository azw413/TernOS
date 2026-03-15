#!/bin/zsh
set -euo pipefail
source /Users/andrew/export-esp.sh
ROOT=/Users/andrew/embedded/TernReader
IDF_ROOT="$ROOT/.embuild/espressif/esp-idf/v5.3.2"
PYENV="$ROOT/.embuild/espressif/python_env/idf5.3_py3.10_env/bin/python"
IDFPY="$IDF_ROOT/tools/idf.py"
ESP_GCC_ROOT="$ROOT/.embuild/espressif/tools/xtensa-esp-elf/esp-13.2.0_20240530/xtensa-esp-elf/bin"
PORT="${PORT:-/dev/cu.usbserial-0214212B}"
MONITOR="${MONITOR:-0}"

cd "$ROOT/m5paper"
cargo +esp build --release

export TERN_RUST_LIB_PATH="$ROOT/target/xtensa-esp32-espidf/release/libtern_m5paper.a"
export IDF_COMPONENT_MANAGER=0
export IDF_PATH="$IDF_ROOT"
export IDF_TOOLS_PATH="$ROOT/.embuild/espressif"
export IDF_PYTHON_ENV_PATH="$ROOT/.embuild/espressif/python_env/idf5.3_py3.10_env"
export PATH="$ESP_GCC_ROOT:$PATH"
export CC="$ESP_GCC_ROOT/xtensa-esp32-elf-gcc"
export CXX="$ESP_GCC_ROOT/xtensa-esp32-elf-g++"
export ASM="$ESP_GCC_ROOT/xtensa-esp32-elf-gcc"

cd "$ROOT/m5paper/espidf"
rm -rf build
"$PYENV" "$IDFPY" -p "$PORT" build flash

if [[ "$MONITOR" == "1" ]]; then
  "$PYENV" "$IDFPY" -p "$PORT" monitor
else
  echo
  echo "Flashed successfully."
  echo "To monitor:"
  echo "  cd $ROOT/m5paper/espidf && $PYENV $IDFPY -p $PORT monitor"
fi
