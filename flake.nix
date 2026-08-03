{
  description = "GTK4 Layer-Shell Audio and Brightness Control Applet";

  nixConfig = {
    extra-substituters = [
      "https://audio-brightness-applet.cachix.org"
    ];
    extra-trusted-public-keys = [
      "audio-brightness-applet.cachix.org-1:RCWMTeprJ+27QtkEg/0fbEkoDtI/Zqc8vUG/APXmJJw="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        buildInputs = with pkgs; [
          gtk4
          gtk4-layer-shell
          libpulseaudio
          glib
        ];

        nativeBuildInputs = with pkgs; [
          pkg-config
          rustc
          cargo
          wrapGAppsHook4
        ];
      in
      {
        # Build Output (`nix build`)
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "audio-brightness-applet";
          version = "1.0.1";
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          inherit buildInputs nativeBuildInputs;

          postInstall = ''
            wrapProgram $out/bin/audio-brightness-applet \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.brightnessctl ]}
          '';
        };

        # App Runner Output (`nix run`)
        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/audio-brightness-applet";
        };

        # Shell for Development (`nix develop`)
        devShells.default = pkgs.mkShell {
          inherit buildInputs;

          nativeBuildInputs = nativeBuildInputs ++ (with pkgs; [
            rust-analyzer
            clippy
            rustfmt
            brightnessctl
          ]);

          RUST_SRC_PATH = "${pkgs.rustc.src}/library";
        };
      }
    );
}
