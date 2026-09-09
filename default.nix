# Plain-Nix entry point (no flakes).
#
#   nix-build            build the package into ./result
#   nix-env -f . -i      install it into the user profile
#

{ stdenv, lib, makeWrapper }:

stdenv.mkDerivation {
  pname = "gbcalc";
  version = "0.1.0"; # adjust or use a git-based version if you like

  src = ./.;

  # If you have a Makefile with a default target that builds `gbcalc`:
  buildPhase = ''
    runHook preBuild
    make
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/bin
    install -m755 gbcalc $out/bin/gbcalc

    runHook postInstall
  '';

  meta = {
    description = "gbcalc – your C calculator tool";
    license = lib.licenses.mit; # or whatever license you use
    platforms = lib.platforms.linux;
  };
}
