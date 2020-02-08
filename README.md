wg-bond
=======

# All of the following is a draft, and is not actually functionality at the moment!

Easy configuration of new Wireguard clients.

### Setup

On a server:

##### Initialize a config:

```shell script
$ wgbond init interface-name
```

It will create wgbond.json in a current directory.

##### Set your wireguard server ip address:

```shell script
$ wgbond set address=[address]
```

##### Bind another computer securely over HTTP

IANA is evil, HTTPS is broken, yada yada.

On a server:

```shell script
$ wgbond add onetime
curl http://[address]/SoM3t0K3n/interface-name.conf | unzip -P 'A/s3kr3t/C0D3'
```

Run this command, and you'll get a configuration.
You also can get a Nix configuration by substituting `.conf` by `.nix` in a URL.

##### Bind via QR code

On a server:

```shell script
$ wgbond add qr
# ### ## IMAGINE A QR CODE HERE #### # ## #
```

Scan a qr code, and you are good to go.