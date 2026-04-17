{
  description = "BitDB dev shell — Rust + pandoc/beamer/LaTeX for slides";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # Minimal TeX live subset: beamer + fonts + tikz — keeps the closure small.
        tex = pkgs.texlive.combine {
          inherit (pkgs.texlive)
            scheme-small
            beamer
            pgf          # tikz
            fontspec
            lm           # Latin Modern (default beamer font)
            ;
        };
      in {
        devShells.default = pkgs.mkShell {
          name = "bitdb";
          packages = [
            # Slide toolchain
            pkgs.pandoc
            tex

            # Rust toolchain (if rustup is not already on PATH)
            pkgs.rustup

            # task runner
            pkgs.just
          ];
        };
      });
}
