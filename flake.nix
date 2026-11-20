{
  description = "magit-lsp development and testing environments";

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
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "magit-lsp";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            cargoLock.lockFile = ./Cargo.lock;
            meta = {
              description = "A Git repository interface for editors over the Language Server Protocol";
              license = pkgs.lib.licenses.gpl3Only;
              mainProgram = "magit-lsp";
            };
          };
        });

      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          llvm = pkgs.rustPlatform.rust.rustc.llvmPackages.llvm;

          # Development tools, excluding test editors.
          dev = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              git
              gnumake
              lua5_4
              lua-language-server
              nodejs
              pnpm_10
              python3
              pyright
              rust-analyzer
              rustc
              rustfmt
              stylua
              typescript-language-server
              ps  # used for tests to check child processes

              # Development-only checking tools
              cargo-machete
              lua51Packages.luacheck
              # jscpd.packages.${system}.default
              actionlint # Check github action workflows
              cargo-llvm-cov
            ];

            shellHook = ''
              export RUST_SRC_PATH="${pkgs.rustPlatform.rustLibSrc}"
              export LLVM_COV="${llvm}/bin/llvm-cov"
              export LLVM_PROFDATA="${llvm}/bin/llvm-profdata"
              echo "magit-lsp dev shell"
              export pnpm_config_pm_on_fail=ignore
            '';
          };

          # Adds test editors, without VSCodium.
          test = pkgs.mkShell {
            inputsFrom = [ dev ];

            packages = with pkgs; [
              neovim
              helix
              tmux
            ];
          };

          # VSCodium is opt-in.
          vscode = pkgs.mkShell {
            inputsFrom = [ test ];

            packages = with pkgs; [
              vscodium
            ] ++ lib.optionals stdenv.isLinux [
              xdotool # X11 input for the A06 e2e scenario
            ];
          };
        in
        {
          # The default shell is the complete local development and test setup;
          # keep `nix develop .#dev` available for a smaller tool-only shell.
          default = test;
          inherit dev;
          inherit test vscode;
        });
    };
}
