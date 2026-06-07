{
  description = "exos - bare-metal microkernel for aarch64";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        # Pinned Rust toolchain with the targets and components the
        # kernel build needs. Change here, get the same toolchain
        # everywhere.
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "aarch64-unknown-none" ];
          extensions = [ "rust-src" "llvm-tools-preview" "rust-analyzer" ];
        };

        # cargo build --release + llvm-objcopy into build/Image.
        buildImage = pkgs.writeShellScriptBin "exos-build" ''
          set -euo pipefail
          export PATH="${rustToolchain}/bin:${pkgs.llvm}/bin:$PATH"
          cargo build --release
          mkdir -p build
          llvm-objcopy -O binary \
            target/aarch64-unknown-none/release/kernel \
            build/Image
          echo "image: build/Image"
        '';

        # qemu-system-aarch64 with our standard flags. Extra arguments
        # are forwarded to QEMU.
        runQemu = pkgs.writeShellScriptBin "exos-run" ''
          set -euo pipefail
          ${self.packages.${system}.build}/bin/exos-build
          exec ${pkgs.qemu}/bin/qemu-system-aarch64 \
            -M virt -cpu cortex-a72 -smp 4 -m 128M \
            -nographic -kernel build/Image "$@"
        '';

        # `cargo clean` plus removing the raw image.
        cleanAll = pkgs.writeShellScriptBin "exos-clean" ''
          set -euo pipefail
          export PATH="${rustToolchain}/bin:$PATH"
          cargo clean
          rm -rf build
        '';
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.llvm
            pkgs.qemu
          ];

          shellHook = ''
            echo "exos dev shell"
            echo "  rustc:        $(rustc --version)"
            echo "  llvm-objcopy: $(llvm-objcopy --version | head -1)"
            echo "  qemu:         $(qemu-system-aarch64 --version | head -1)"
            echo ""
            echo "Run with:    nix run .#run   (or .#build, .#clean)"
            echo "Cargo aware: cargo build --release"
          '';
        };

        packages.build = buildImage;
        packages.run = runQemu;
        packages.clean = cleanAll;
        packages.default = runQemu;

        apps.build = {
          type = "app";
          program = "${buildImage}/bin/exos-build";
        };
        apps.run = {
          type = "app";
          program = "${runQemu}/bin/exos-run";
        };
        apps.clean = {
          type = "app";
          program = "${cleanAll}/bin/exos-clean";
        };
        apps.default = self.apps.${system}.run;
      });
}
