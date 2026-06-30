.PHONY: build build-release image run run-release clean shell help

help:
	@echo "preon make targets (thin wrappers around flake.nix):"
	@echo "  make build          nix run .#build          — dev image (debug-assertions + overflow-checks)"
	@echo "  make build-release  nix run .#build-release  — clean release image"
	@echo "  make image          alias for build"
	@echo "  make run            nix run .#run            — dev image + qemu (asserts on)"
	@echo "  make run-release    nix run .#run-release    — release image + qemu"
	@echo "  make clean          nix run .#clean          — cargo clean + rm build/"
	@echo "  make shell          nix develop              — interactive dev shell"
	@echo ""
	@echo "Direct cargo / qemu usage is fine too; flake just pins tooling."

build:
	nix run .#build

build-release:
	nix run .#build-release

image: build

run:
	nix run .#run

run-release:
	nix run .#run-release

clean:
	nix run .#clean

shell:
	nix develop
