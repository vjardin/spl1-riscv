sudo apt install cargo
sudo apt install rustup
rustup toolchain install stable
rustup default stable
rustup target add riscv64imac-unknown-none-elf
sudo apt install gcc-riscv64-unknown-elf
sudo apt install qemu-system-misc

cargo build

qemu-system-riscv64 \
  -M virt \
  -m 256M \
  -bios none \
  -kernel target/riscv64imac-unknown-none-elf/debug/spl1-riscv \
  -display none \
  -serial stdio \
  -monitor none
