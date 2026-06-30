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

        # cargo build (for the given profile) + llvm-objcopy into build/Image.
        # `cargoFlag` selects the profile; `subdir` is the matching target
        # output directory.
        mkBuild = { name, cargoFlag, subdir }:
          pkgs.writeShellScriptBin name ''
            set -euo pipefail
            export PATH="${rustToolchain}/bin:${pkgs.llvmPackages_latest.llvm}/bin:$PATH"
            cargo build ${cargoFlag}
            mkdir -p build
            llvm-objcopy -O binary \
              target/aarch64-unknown-none/${subdir}/kernel \
              build/Image
            echo "image: build/Image (${subdir})"
          '';

        # Dev image carries debug-assertions + overflow-checks (see the
        # release-dev profile in Cargo.toml); release is the clean image.
        buildDev = mkBuild {
          name = "exos-build";
          cargoFlag = "--profile release-dev";
          subdir = "release-dev";
        };
        buildRelease = mkBuild {
          name = "exos-build-release";
          cargoFlag = "--release";
          subdir = "release";
        };

        # qemu-system-aarch64 with our standard flags. Builds the chosen
        # image first; extra arguments are forwarded to QEMU.
        mkRun = { name, build }:
          pkgs.writeShellScriptBin name ''
            set -euo pipefail
            ${pkgs.lib.getExe build}
            exec ${pkgs.qemu}/bin/qemu-system-aarch64 \
              -M virt -cpu cortex-a72 -smp 4 -m 2G \
              -nographic -kernel build/Image "$@"
          '';

        runDev = mkRun { name = "exos-run"; build = buildDev; };
        runRelease = mkRun { name = "exos-run-release"; build = buildRelease; };

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
            pkgs.llvmPackages_latest.llvm
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

        packages.build = buildDev;
        packages.build-release = buildRelease;
        packages.run = runDev;
        packages.run-release = runRelease;
        packages.clean = cleanAll;
        packages.default = runDev;

        apps.build = {
          type = "app";
          program = pkgs.lib.getExe buildDev;
        };
        apps.build-release = {
          type = "app";
          program = pkgs.lib.getExe buildRelease;
        };
        apps.run = {
          type = "app";
          program = pkgs.lib.getExe runDev;
        };
        apps.run-release = {
          type = "app";
          program = pkgs.lib.getExe runRelease;
        };
        apps.clean = {
          type = "app";
          program = pkgs.lib.getExe cleanAll;
        };
        apps.default = self.apps.${system}.run;
      });
}
