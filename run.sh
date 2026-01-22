#!/bin/bash
set -euox pipefail

cargo espflash save-image --release --chip=esp32c3 firmware.bin
cargo espflash write-bin 0x10000 firmware.bin --monitor
