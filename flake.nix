{
  description = "A devShell example";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      {
        devShells.default =
          with pkgs;
          mkShell {
            buildInputs = [
              rust-bin.beta.latest.default
              pkg-config
              alsa-lib
            ];
          };

        devShells.windows =
          let
            cross = pkgs.pkgsCross.mingwW64;
            prefix = cross.stdenv.cc.targetPrefix;
          in
          pkgs.mkShell {
            packages = [
              (pkgs.rust-bin.beta.latest.default.override {
                targets = [ "x86_64-pc-windows-gnu" ];
              })
              cross.stdenv.cc
              cross.stdenv.cc.bintools
            ];

            CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${prefix}gcc";
            CC_x86_64_pc_windows_gnu = "${prefix}gcc";
            AR_x86_64_pc_windows_gnu = "${prefix}ar";
            CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS =
              "-L native=${pkgs.lib.getLib cross.windows.pthreads}/lib";
          };
      }
    );
}
