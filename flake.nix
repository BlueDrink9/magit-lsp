{
  description = "magit-lsp development shell with Rust, Lua and Node LSP servers";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
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
              nodejs
              python3
              rust-analyzer
              rustc
              rustfmt
              tmux
              typescript-language-server
            ];

            shellHook = ''
              export RUST_SRC_PATH="${pkgs.rustPlatform.rustLibSrc}"
              echo "magit-lsp dev shell"
              echo "  rust-analyzer:            $(command -v rust-analyzer)"
              echo "  lua-language-server:      $(command -v lua-language-server)"
              echo "  typescript-language-server: $(command -v typescript-language-server)"
            '';
          };
        });
    };
}
