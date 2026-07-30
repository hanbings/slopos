SHELL := /bin/bash
CARGO := $(shell command -v cargo 2>/dev/null || echo "$$HOME/.cargo/bin/cargo")

.PHONY: all build image run test-boot test-interaction clean

all: build

build:
	$(CARGO) build --locked --release -p slopos-kernel --target x86_64-unknown-none
	$(CARGO) build --locked --release -p slopos-boot --target x86_64-unknown-uefi

image: build
	./scripts/make-image.sh

run: image
	./scripts/run-qemu.sh

test-boot: image
	./scripts/test-boot.sh

test-interaction: image
	./scripts/test-interaction.sh

clean:
	$(CARGO) clean
