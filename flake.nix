{
  description = "nohrs devshell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        linuxDeps = pkgs.lib.optionals pkgs.stdenv.isLinux (
          with pkgs; [
            libxkbcommon wayland mesa libGL
            libxcb libx11 libxcursor libxi
            vulkan-loader vulkan-headers
          ]
        );
        libPath = pkgs.lib.makeLibraryPath (
          (with pkgs; [ fontconfig freetype openssl ]) ++ linuxDeps
        );
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust
            pkg-config
            openssl
            fontconfig
            freetype
            cargo-llvm-cov
            cargo-deny
            cargo-machete
            typos
          ] ++ linuxDeps;

          LD_LIBRARY_PATH = libPath;
        };
      });
}
