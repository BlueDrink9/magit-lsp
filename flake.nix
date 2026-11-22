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
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
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

          # Release compilation: pinned Rust/Cargo, but the native OS
          # supplies the linker, C runtime and SDK.
          #
          # Intentionally does NOT inherit from dev/test:
          # no Nix C compiler wrapper and no test editors.
          releaseNative = pkgs.mkShellNoCC {
            packages = with pkgs; [
              rustc
              cargo
              git
              gnumake
              python3
            ];
          };

          # Functional acceptance of already-built release artifacts.
          # This shell intentionally has no Rust compiler or Cargo.
          #
          # Clean portability smoke tests run separately on hosted
          # machines without Nix.
          releaseAcceptance = pkgs.mkShellNoCC {
            packages = with pkgs; [
              git
              python3
              neovim
              tmux
              nodejs_24
              pnpm_10
            ];
          };

          # Development tools, excluding test editors.
          dev = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              git
              gnumake
              lua5_4
              lua-language-server
              nodejs_24
              pnpm_10
              python3
              pyright
              rust-analyzer
              rustc
              rustfmt
              shellcheck
              stylua
              typescript-language-server
              ps  # used for tests to check child processes

              # Development-only checking tools
              cargo-machete
              lua51Packages.luacheck
              # jscpd.packages.${system}.default
              actionlint # Check github action workflows
              cargo-llvm-cov

              perf
              strace
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
          release-native = releaseNative;
          release-acceptance = releaseAcceptance;
        });
    };
}
