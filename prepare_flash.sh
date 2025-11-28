#!/usr/bin/env bash
set -euo pipefail

# Build SPL1 (Rust) and prepare a 32 MiB NOR pflash image (pflash0.img)
# for QEMU "virt" where:
#   - SPL1 executes in place from 0x2000_0000 (pflash0)
#   - Boot metadata (counters) live in the last 128 KiB block

FLASH_SIZE_MB=32
FLASH_SIZE=$((FLASH_SIZE_MB * 1024 * 1024))
BLOCK_SIZE=$((128 * 1024)) # 128 KiB
FLASH_IMG="pflash0.img"
TARGET_TRIPLE="riscv64imac-unknown-none-elf"
PROFILE="release" # or "debug"

ELF="target/${TARGET_TRIPLE}/${PROFILE}/spl1-riscv"
BIN="spl1.bin"

# Where boot metadata lives: last block of flash
META_OFFSET=$((FLASH_SIZE - BLOCK_SIZE))

echo "=== Building SPL1 (${PROFILE}) for ${TARGET_TRIPLE} ==="
cargo build --target "${TARGET_TRIPLE}" --${PROFILE}

if [[ ! -f "${ELF}" ]]; then
  echo "ERROR: ELF not found at ${ELF}" >&2
  exit 1
fi

echo "=== Converting ELF to raw binary ==="
riscv64-unknown-elf-objcopy -O binary "${ELF}" "${BIN}"

BIN_SIZE=$(stat -c '%s' "${BIN}")
echo "SPL1 binary size: ${BIN_SIZE} bytes"

if (( BIN_SIZE > META_OFFSET )); then
  echo "ERROR: SPL binary (${BIN_SIZE} bytes) overlaps metadata block (starts at ${META_OFFSET})." >&2
  exit 1
fi

echo "=== Creating ${FLASH_SIZE_MB} MiB flash image filled with 0xFF ==="
dd if=/dev/zero bs=1M count="${FLASH_SIZE_MB}" status=none | \
  tr '\000' '\377' > "${FLASH_IMG}"

echo "=== Writing SPL1 at flash offset 0x00000000 ==="
dd if="${BIN}" of="${FLASH_IMG}" bs=1 conv=notrunc status=none

echo "=== Ensuring metadata block (last 128 KiB) is erased (0xFF) ==="
dd if=/dev/zero bs="${BLOCK_SIZE}" count=1 status=none | \
  tr '\000' '\377' | \
  dd of="${FLASH_IMG}" bs=1 seek="${META_OFFSET}" conv=notrunc status=none

echo
echo "Done. Generated flash image: ${FLASH_IMG}"
echo "  - size        : ${FLASH_SIZE_MB} MiB"
echo "  - meta offset : ${META_OFFSET} (0x$(printf '%x' "${META_OFFSET}"))"
echo
echo "Run QEMU like this to boot SPL1 directly from pflash0:"
echo "  qemu-system-riscv64 \\"
echo "    -M virt \\"
echo "    -m 256M \\"
echo "    -bios none \\"
echo "    -drive if=pflash,format=raw,unit=0,file=${FLASH_IMG},readonly=off \\"
echo "    -display none -serial stdio -monitor none"
