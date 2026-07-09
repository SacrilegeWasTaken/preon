.PHONY: build build-release image run run-release clean shell verify help

help:
	@echo "preon make targets (thin wrappers around flake.nix):"
	@echo "  make build          nix run .#build          — dev image (debug-assertions + overflow-checks)"
	@echo "  make build-release  nix run .#build-release  — clean release image"
	@echo "  make image          alias for build"
	@echo "  make run            nix run .#run            — dev image + qemu (asserts on)"
	@echo "  make run-release    nix run .#run-release    — release image + qemu"
	@echo "  make clean          nix run .#clean          — cargo clean + rm build/"
	@echo "  make shell          nix develop              — interactive dev shell"
	@echo "  make verify         cargo kani (host model)  — formal-verification harnesses"
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

# Formal verification — Kani model-checking harnesses over the pure logic
# cores (address/index math today; buddy allocator next). Harnesses are
# #[cfg(kani)] and invisible to `cargo build`, so this never touches the
# kernel image.
#
# One-time prereq (outside Nix; Kani ships its own CBMC + rustc):
#   cargo install --locked kani-verifier && cargo kani setup
#
# Kani analyses on the *host* machine model, so we override the workspace's
# forced aarch64-unknown-none target with the detected host triple. If Kani
# rejects the override on first run, fall back to a proofs crate outside the
# root .cargo/config.toml (see docs/VERIFICATION.md).
KANI_TARGET ?= $(shell rustc -vV | sed -n 's/host: //p')

verify:
	CARGO_BUILD_TARGET=$(KANI_TARGET) cargo kani -p kernel_arch -p kernel_mm
