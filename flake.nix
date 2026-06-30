{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustPackage = pkgs.rust-bin.stable."1.85.0".default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };
        fhs = pkgs.buildFHSUserEnv {
          name = "rust-fhs-shell";
          targetPkgs = pkgs: (with pkgs; [
            rustPackage
            
            # Compilers and build tools
            gcc
            clang_19
            
            # Linkers
            mold
            binutils # ld, ld.bfd, ld.gold
            lld_19   # ld.lld
            
            # Libraries and dependencies
            openssl
            openssl.dev
            pkg-config
            protobuf_27
            
            # System libraries
            zlib
            zlib.dev

            # Python environment for plot.py
            (python3.withPackages (ps: with ps; [
              pandas
              matplotlib
              numpy
              seaborn
            ]))
          ]);
          
          profile = ''
            export RUSTC_VERSION=1.85.0
            export RUSTFLAGS="-C target-cpu=native"
            export RUST_BACKTRACE=1
          '';
        };
      in {
        devShells.default = fhs.env;
      }
    );
}