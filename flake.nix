{
  description = "magit-lsp development shell with Rust, Lua, Node and Python LSP servers";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  # inputs.jscpd.url = "github:kucherenko/jscpd";
  # inputs.jscpd.inputs.nixpkgs.follows = "nixpkgs";

  outputs = { self
  , nixpkgs
  # , jscpd
  }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              git
              gnumake
              lua5_4
              lua-language-server
              neovim
              helix
              vscodium
              nodejs
              pnpm
              python3
              pyright
              rust-analyzer
              rustc
              rustfmt
              tmux
              typescript-language-server
              cargo-machete
              lua51Packages.luacheck
              # jscpd.packages.${system}.default
            ];

            shellHook = ''
              export RUST_SRC_PATH="${pkgs.rustPlatform.rustLibSrc}"
              echo "magit-lsp dev shell"
            '';
          };
        });
    };
}
