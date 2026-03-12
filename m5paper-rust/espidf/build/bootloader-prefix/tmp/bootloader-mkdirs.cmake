# Distributed under the OSI-approved BSD 3-Clause License.  See accompanying
# file Copyright.txt or https://cmake.org/licensing for details.

cmake_minimum_required(VERSION 3.5)

file(MAKE_DIRECTORY
  "/Users/andrew/embedded/TernReader/.embuild/espressif/esp-idf/v5.3.2/components/bootloader/subproject"
  "/Users/andrew/embedded/TernReader/m5paper-rust/espidf/build/bootloader"
  "/Users/andrew/embedded/TernReader/m5paper-rust/espidf/build/bootloader-prefix"
  "/Users/andrew/embedded/TernReader/m5paper-rust/espidf/build/bootloader-prefix/tmp"
  "/Users/andrew/embedded/TernReader/m5paper-rust/espidf/build/bootloader-prefix/src/bootloader-stamp"
  "/Users/andrew/embedded/TernReader/m5paper-rust/espidf/build/bootloader-prefix/src"
  "/Users/andrew/embedded/TernReader/m5paper-rust/espidf/build/bootloader-prefix/src/bootloader-stamp"
)

set(configSubDirs )
foreach(subDir IN LISTS configSubDirs)
    file(MAKE_DIRECTORY "/Users/andrew/embedded/TernReader/m5paper-rust/espidf/build/bootloader-prefix/src/bootloader-stamp/${subDir}")
endforeach()
if(cfgdir)
  file(MAKE_DIRECTORY "/Users/andrew/embedded/TernReader/m5paper-rust/espidf/build/bootloader-prefix/src/bootloader-stamp${cfgdir}") # cfgdir has leading slash
endif()
