{ pkgs, lib, config, inputs, ... }:

{
  packages = [ pkgs.git ];

  languages.rust.enable = true;

  # See full reference at https://devenv.sh/reference/options/
}
