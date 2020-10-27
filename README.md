wg-bond
=======

Easy Wireguard configurator.

![preview](./peek.gif)

I didn't write any documentation yet, but `--help` option is available.

## NixOps example

```bash

# Initialize a config:
wgbond init wgvpn

# Add server:
wgbond add server \
--endpoint example.com:42000 \
--nixops \          # Include into NixOps export
--center \          # Make clients use this peer as gateway
--gateway \         # And get internet through it
--masquerade eth0   # And forward via eth0

wgbond add phone --keepalive 30

# Generate and push config to your NixOps cluster
wgbond nixops > wg.nix
nixops modify wg.nix machines.nix
nixops deploy

# Generate config for your phone
wgbond qr phone

```

## Developing

Use [VSCodium](https://vscodium.com/) + [Nix](https://nixos.org/nix) for the best experience.
Just add recommended extensions, and you are good to go.
