wg-bond
=======

Easy Wireguard configurator.

![preview](./peek.gif)


### Setup

#### Initialize a config:

```shell script
$ wgbond init wgvpn
```

It will create wg-bond.json in a current directory.


#### Add peers:

```shell script
$ wgbond add testpeer --endpoint=example.com:42000
```

#### Generate configs:

```shell script
$ wgbond conf 1 # Referencing hosts by name is not here yet
```

#### Set your wireguard server ip address:

```shell script
$ wgbond edit [ID] --endpoint=[address]
```


# All of the following is a draft, and is not actual functionality at the moment!

##### Bind another computer securely over HTTP

IANA is evil, HTTPS is broken, yada yada.

On a server:

```shell script
$ wgbond onetime
Starting server...
curl http://[address]/SoM3t0K3n/interface-name.conf | unzip -P 'A/s3kr3t/C0D3'
```

Run this command, and you'll get a configuration.
You also can get a Nix configuration by substituting `.conf` by `.nix` in a URL.