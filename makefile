.PHONY: build image run clean shell help

help:
	@echo "exos make targets (thin wrappers around flake.nix):"
	@echo "  make build    nix run .#build   — cargo + llvm-objcopy → build/Image"
	@echo "  make image    alias for build"
	@echo "  make run      nix run .#run     — build + qemu"
	@echo "  make clean    nix run .#clean   — cargo clean + rm build/"
	@echo "  make shell    nix develop       — interactive dev shell"
	@echo ""
	@echo "Direct cargo / qemu usage is fine too; flake just pins tooling."

build:
	nix run .#build

image: build

run:
	nix run .#run

clean:
	nix run .#clean

shell:
	nix develop
