.PHONY: build image run-qemu run clean

BUILD_DIR := build
KERNEL_ELF := target/aarch64-unknown-none/release/kernel
KERNEL_IMG := $(BUILD_DIR)/Image

build:
	cargo build --release

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

image: $(BUILD_DIR) build
	nix shell nixpkgs#llvm --command \
		llvm-objcopy -O binary $(KERNEL_ELF) $(KERNEL_IMG)

run-qemu: image
	qemu-system-aarch64 \
		-M virt \
		-cpu cortex-a72 \
		-m 128M \
		-nographic \
		-kernel $(KERNEL_IMG)

run: run-qemu

clean:
	rm -rf $(BUILD_DIR)
