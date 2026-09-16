# Plain-Nix entry point (no flakes).
#
#   nix-build            build the package into ./result
#   nix-env -f . -i      install it into the user profile
#

{ rustPlatform, lib }:

rustPlatform.buildRustPackage {
  pname = "gbcalc";
  version = "0.1.0"; # adjust or use a git-based version if you like

  src = ./.;

  cargoLock.lockFile = ./Cargo.lock;

  # Baked in at compile time, read back by src/theme.rs via option_env!, so
  # the installed binary finds its themes without needing a wrapper script.
  GBCALC_THEMEDIR = "${placeholder "out"}/share/gbcalc/themes";

  postInstall = ''
    install -d $out/share/gbcalc/themes
    install -m644 themes/*.conf $out/share/gbcalc/themes/
  '';

  meta = {
    description = "gbcalc – a TUI scientific calculator";
    license = lib.licenses.mit; # or whatever license you use
    platforms = lib.platforms.linux;
  };
}
