# rust based RISCV SPL1

It is just a PoC, do not use it.

```bash
sudo apt install cargo
sudo apt install rustup
rustup toolchain install stable
rustup default stable
rustup target add riscv64imac-unknown-none-elf
sudo apt install gcc-riscv64-unknown-elf
sudo apt install qemu-system-misc
```

Build:
```bash
cargo build
```

Run:
```bash
qemu-system-riscv64 \
  -M virt \
  -m 256M \
  -bios none \
  -kernel target/riscv64imac-unknown-none-elf/debug/spl1-riscv \
  -display none \
  -serial stdio \
  -monitor none
```
