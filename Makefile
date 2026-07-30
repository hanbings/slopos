SHELL := /bin/bash
CARGO := $(shell command -v cargo 2>/dev/null || echo "$$HOME/.cargo/bin/cargo")

.PHONY: all build rootfs image run test-acpi test-ebpf test-pci test-virtio test-ext4 test-vfs test-boot test-interaction test-page-fault clean

all: build

build:
	$(CARGO) build --locked --release -p slopos-kernel --target x86_64-unknown-none
	$(CARGO) build --locked --release -p slopos-boot --target x86_64-unknown-uefi

rootfs:
	./scripts/make-rootfs.sh

image: build rootfs
	./scripts/make-image.sh

run: image
	./scripts/run-qemu.sh

test-acpi:
	$(CARGO) test --locked -p slopos-acpi

test-ebpf:
	$(CARGO) test --locked -p slopos-ebpf

test-pci:
	$(CARGO) test --locked -p slopos-pci

test-virtio:
	$(CARGO) test --locked -p slopos-virtio

test-ext4:
	$(CARGO) test --locked -p slopos-ext4

test-vfs:
	$(CARGO) test --locked -p slopos-vfs

test-boot: image
	./scripts/test-boot.sh

test-interaction: image
	./scripts/test-interaction.sh

test-page-fault: image
	./scripts/test-page-fault.sh

clean:
	$(CARGO) clean
