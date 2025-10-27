#!/usr/bin/env -S deno run --allow-run --allow-read --allow-write --allow-env
// AWG configuration manager
// TODO: Add wg compat (in genkey etc methods)
//

import { parser } from "@exodus/schemasafe";
import { Address4, Address6 } from "ip-address";
import { exec } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";

type PeerConfig = {
  Address: string;
  AllowedIPs: string[];
};

type AWGInterfaceInfo = {
  Jc: string;
  Jmin: string;
  Jmax: string;
  S1: string;
  S2: string;
  H1: string;
  H2: string;
  H3: string;
  H4: string;
};

type WGInterfaceInfo = {
  PrivateKey: string;
  ListenPort?: number;
  Address?: string[];
  Table?: number;
  PreUp?: string[];
  PreDown?: string[];
  PostUp?: string[];
  PostDown?: string[];
};

type WgPeerInfo = {
  PublicKey: string;
  PresharedKey: string;
  AllowedIPs: string[];
  Endpoint?: string;
  PersistentKeepalive?: number;
};

type AWGPeerInfo = {
  PublicKey: string;
  PresharedKey: string;
  AllowedIPs?: string[];
  Endpoint?: string;
  PersistentKeepalive?: number;
};

type FullConfig = {
  iface: WGInterfaceInfo & (AWGInterfaceInfo | {});
  peers: { [name: string]: AWGPeerInfo };
};

type Config = {
  name: string;
  hosts: HostInfo[];
};

type HostInfo = {
  id: number;
  endpoint?: string;
  tags: string[];
  port?: number;
};

type HostInfoNamed = HostInfo & {
  name: string;
};

type ConnectionInfo = {
  action: string;
  from: string;
  to: string;
};

type EncloseIP<In> = In extends Address4
  ? Address4
  : In extends Address6
    ? Address6
    : never;

type InterfaceOptionProperty = {
  action: "interface-option";
} & {
  [K in keyof WGInterfaceInfo]: { name: K; value: WGInterfaceInfo[K] };
}[keyof WGInterfaceInfo];

type Property = { target: string } & (
  | InterfaceOptionProperty
  | { action: "masquerade" }
  | { action: "assign-ip"; cidr: string }
);

type Relation = {
  from: string;
  to: string;
} & (
  | {
      action: "proxy";
      cidr: string;
    }
  | {
      action: "keepalive";
      time: number;
    }
  | {
      action: "mention";
    }
);

export type NetworkConfig = {
  $schema?: "./config.schema.json" | undefined;
  network: {
    interface_name: string;
    awg?: AWGInterfaceInfo;
  };
  hosts: { [index: string]: HostInfo };
  relations: Relation[];
  properties: Property[];
};

type SecretStorage = {
  PrivateKeys: { [name: string]: string };
  PSKs: { [name: string]: string };
};

function parseIPAddress(ipString: string): Address4 | Address6 {
  try {
    // Try parsing as IPv6 first
    return new Address6(ipString);
  } catch (e) {
    try {
      // Try parsing as IPv4
      return new Address4(ipString);
    } catch (e2) {
      console.log(e2);
      throw new Error(`Invalid IP address: ${ipString}`);
    }
  }
}

function getIpInCIDR<A extends EncloseIP<any>>(
  ipAddress: A,
  id: number,
): EncloseIP<A> {
  let ret;
  switch (true) {
    case ipAddress instanceof Address4:
      {
        ret = Address4.fromBigInt(
          ipAddress.bigInt() + BigInt(id),
        ) as EncloseIP<A>;
        ret.subnetMask = ipAddress.subnetMask;
      }
      break;
    case ipAddress instanceof Address6:
      {
        ret = Address6.fromBigInt(
          ipAddress.bigInt() + BigInt(id),
        ) as EncloseIP<A>;
        ret.subnetMask = ipAddress.subnetMask;
      }
      break;
    default:
      throw new Error(`Unsupported IP address type: ${ipAddress}`);
  }
  if (!ipAddress.isInSubnet(ret)) {
    throw new Error(
      `CIDR ${ipAddress.address} (/${ipAddress.subnetMask}) is too small for host ${id}`,
    );
  }
  return ret;
}

function rand_incl(a: number, b: number): number {
  return Math.floor((b - a + 1) * Math.random() + a);
}

function writeSection(section: {
  name: string;
  comment?: string;
  kv: { [name: string]: string | string[] | number };
}) {
  function writeKey(value: string | number) {
    switch (true) {
      case typeof value === "number":
        return value.toString();
      default:
        return value;
    }
  }
  function writeKV(key: string, value: string | number | undefined | string[]) {
    switch (true) {
      case value === undefined:
        return undefined;
      case Array.isArray(value):
        return value.map((value) => `${key} = ${writeKey(value)}`).join("\n");
      default:
        return `${key} = ${writeKey(value)}`;
    }
  }
  return `[${section.name}] # ${section.comment ?? ""}\n${Object.keys(
    section.kv,
  )
    .map((key) => writeKV(key, section.kv[key]!))
    .filter((it) => it)
    .join("\n")}`;
}

function writeConfig({ iface, peers }: FullConfig, comment?: string) {
  return `${writeSection({ name: "Interface", comment, kv: iface })}\n\n${Object.keys(
    peers,
  )
    .map((peer_name) =>
      writeSection({ name: "Peer", kv: peers[peer_name]!, comment: peer_name }),
    )
    .join("\n\n")}`;
}

/**
  - Jc — 1 ≤ Jc ≤ 128; recommended range is from 3 to 10 inclusive
  - Jmin — Jmin < Jmax; recommended value is 50
  - Jmax — Jmin < Jmax ≤ 1280; recommended value is 1000
  - S1 — S1 < 1280; S1 + 56 ≠ S2; recommended range is from 15 to 150 inclusive
  - S2 — S2 < 1280; recommended range is from 15 to 150 inclusive
  - H1/H2/H3/H4 — must be unique among each other; recommended range is from 5 to 2147483647 inclusive
*/
function generate_awg_params() {
  let Jc = rand_incl(3, 10);
  let Jmin = 50;
  let Jmax = 1000; // rand_incl(Jmin + 1, 1280)
  let S2 = rand_incl(15, 150);
  var S1 = 0;
  while (S1 === 0) {
    let candidate = rand_incl(15, 150);
    if (candidate + 56 != S2) S1 = candidate;
  }
  let hvals: number[] = [];
  while (hvals.length < 4) {
    let candidate = rand_incl(5, 2147483647);
    if (!hvals.includes(candidate)) hvals.push(candidate);
  }

  return {
    Jc: Jc.toString(),
    Jmin: Jmin.toString(),
    Jmax: Jmax.toString(),
    S1: S1.toString(),
    S2: S2.toString(),
    H1: hvals[0]!.toString(),
    H2: hvals[1]!.toString(),
    H3: hvals[2]!.toString(),
    H4: hvals[3]!.toString(),
  };
}

async function execAsync(cmd: string, stdin?: string): Promise<string> {
  return new Promise((resolve, reject) => {
    let child = exec(cmd, (error, stdout, stderr) => {
      if (!error) {
        resolve(stdout);
      } else {
        reject(error);
      }
    });
    if (stdin !== undefined) {
      child.stdin!.write(stdin);
      child.stdin!.end();
    }
  });
}

var key_binary = "awg";
async function genkey(): Promise<string> {
  return (await execAsync(`${key_binary} genkey`)).trim();
}

async function pubkey(priv: string): Promise<string> {
  return (await execAsync(`${key_binary} pubkey`, priv)).trim();
}

async function genpsk(): Promise<string> {
  return (await execAsync(`${key_binary} genpsk`)).trim();
}

function predicate_matches(predicate: string, host: HostInfoNamed): boolean {
  // Trivial predicate matching logic
  return (
    predicate === "*" || host.tags.includes(predicate) || host.name == predicate
  );
}

// Create all configuration files for all the hosts
async function generate_configurations(
  networkConfig: NetworkConfig,
  secret_storage: SecretStorage,
): Promise<{ [name: string]: FullConfig }> {
  const hosts: HostInfoNamed[] = Object.keys(networkConfig.hosts).map((name) =>
    Object.assign({ name }, networkConfig.hosts[name]),
  );
  const configurations: { [name: string]: FullConfig } = {};
  key_binary = networkConfig.network.awg != undefined ? "awg" : "wg";

  for (const interface_host of hosts) {
    // create a base for the config
    var configuration: FullConfig = {
      iface: Object.assign(
        {
          PrivateKey: await getPrivateKey(interface_host.name),
          ListenPort: interface_host.port,
        },
        networkConfig.network.awg ?? {},
      ),
      peers: {},
    };
    configurations[interface_host.name] = configuration;

    /** Makes sure PSK exists for a given peer pair. */
    async function getPSK(peer_a: string, peer_b: string): Promise<string> {
      const ident = [peer_a, peer_b].sort().join(":");
      if (!(ident in secret_storage.PSKs)) {
        secret_storage.PSKs[ident] = await genpsk();
      }
      return secret_storage.PSKs[ident]!;
    }

    /** Makes sure PrivateKey exists for a given peer. */
    async function getPrivateKey(peer_name: string): Promise<string> {
      const ident = peer_name;
      if (!(ident in secret_storage.PrivateKeys)) {
        secret_storage.PrivateKeys[ident] = await genkey();
      }
      return secret_storage.PrivateKeys[ident]!;
    }

    /** Makes sure peer exists in list of peers for current interface. */
    async function ensurePeer(name: string): Promise<AWGPeerInfo> {
      if (name in configuration.peers) {
        return configuration.peers[name]!;
      } else {
        const host: HostInfoNamed = Object.assign({name}, networkConfig.hosts[name]!);
        let Endpoint: string | undefined;
        if (host.endpoint && host.port) {
          Endpoint = host.endpoint + ":" + host.port;
        }
        const init: AWGPeerInfo = {
          PublicKey: await pubkey(await getPrivateKey(name)),
          PresharedKey: await getPSK(interface_host.name, name),
          Endpoint,
        };

        for (const property of networkConfig.properties) {
          if (predicate_matches(property.target, host)) {
            switch (property.action) {
              case "assign-ip":
                init.AllowedIPs = init.AllowedIPs ?? [];
                init.AllowedIPs.push(
                  getIpInCIDR(
                    parseIPAddress(property.cidr),
                    host.id,
                  ).correctForm(),
                );
                break;
            }
          }
        }
        configuration.peers[name] = init;
        return init;
      }
    }

    for (const property of networkConfig.properties) {
      if (predicate_matches(property.target, interface_host)) {
        switch (property.action) {
          case "assign-ip":
            const ip = getIpInCIDR(
              parseIPAddress(property.cidr),
              interface_host.id,
            ).correctForm();
            configuration.iface.Address = configuration.iface.Address ?? [];
            configuration.iface.Address.push(ip);
            break;
          case "interface-option":
            // err in ts? idk how to better express that in types
            // configuration.iface[property.name] = property.value;
            Object.assign(configuration.iface, {
              [property.name]: property.value,
            });
            break;
          case "masquerade":
            configuration.iface.PreUp = configuration.iface.PreUp ?? [];
            let addmark_rule =
              `PREROUTING -i ${networkConfig.network.interface_name} -j MARK --set-mark 0x30`;
            let masquerade_rule =
              `POSTROUTING ! -o ${networkConfig.network.interface_name} -m mark --mark 0x30 -j MASQUERADE`;
            configuration.iface.PreUp.push(
              `iptables -t mangle -A ${addmark_rule}`,
              `iptables -t nat -A ${masquerade_rule}`,
              `ip6tables -t mangle -A ${addmark_rule}`,
              `ip6tables -t nat -A ${masquerade_rule}`,
            );
            configuration.iface.PostDown = configuration.iface.PostDown ?? [];
            configuration.iface.PostDown.push(
              `iptables -t mangle -D ${addmark_rule}`,
              `iptables -t nat -D ${masquerade_rule}`,
              `ip6tables -t mangle -D ${addmark_rule}`,
              `ip6tables -t nat -D ${masquerade_rule}`,
            );
            break;
        }
      }
    }

    // now that we have one, go through the relations and apply them
    for (const relation of networkConfig.relations) {
      if (predicate_matches(relation.from, interface_host)) {
        const targets = hosts.filter((host) =>
          host.id != interface_host.id && predicate_matches(relation.to, host),
        );
        switch (relation.action) {
          case "proxy":
            // Proxy
            if (targets.length != 1)
              throw new Error(`Proxy target ambiguous (${targets.length})`);
            const peer = await ensurePeer(targets[0]!.name);
            if (!peer.AllowedIPs) peer.AllowedIPs = [];
            peer.AllowedIPs.push(relation.cidr);
            break;
          case "keepalive":
            for (const target of targets) {
              const peer = await ensurePeer(target.name);
              peer.PersistentKeepalive = relation.time;
            }
            break;
          case "mention":
            break;
          default:
            throw new Error(`Unknown relation type: ${relation}`);
        }
      }
      // Right now there are no backward relation edits
      if (predicate_matches(relation.to, interface_host)) {
        const sources = hosts.filter((host) =>
          host.id != interface_host.id && predicate_matches(relation.from, host),
        );
        // need to ensure peers are connected
        for (const source of sources) {
          await ensurePeer(source.name);
        }
        switch (relation.action) {
          case "proxy":
            break;
          case "keepalive":
            break;
          case "mention":
            break;
          default:
            throw new Error(`Unknown relation type: ${relation}`);
        }
      }
    }
  }
  return configurations;
}

// Doesn't generate a correct schema as of yet.
// Some way to add string validation is required.
// let schemaParser = parser(JSON.parse(
//   readFileSync("./config.schema.json", "utf8"),
// ))

// =========== START ===========
let netFilePath = "network_config.json";
let secretsFilePath = "secrets.json";
let exportFolderPath = "configs";

async function generate_configs() {
  const netConfig: NetworkConfig = JSON.parse(
    readFileSync(netFilePath, "utf8"),
  );

  let store: SecretStorage;
  if (existsSync(secretsFilePath)) {
    store = JSON.parse(readFileSync(secretsFilePath, "utf8"));
  } else {
    console.log("Secret file was not found, creating a new one");
    store = { PSKs: {}, PrivateKeys: {} };
  }
  const configs = await generate_configurations(netConfig, store);
  if (!existsSync(exportFolderPath)) mkdirSync(exportFolderPath);
  writeFileSync(secretsFilePath, JSON.stringify(store, null, 2));

  console.log(`Generating host configurations for ${Object.keys(configs).length} host(s)`);
  for (const hostname in configs) {
    if (!existsSync(`${exportFolderPath}/${hostname}`)) mkdirSync(`${exportFolderPath}/${hostname}`);
    const fname = `./${exportFolderPath}/${hostname}/${netConfig.network.interface_name}.conf`;
    console.log(`${hostname} → ${fname}`);
    writeFileSync(fname, writeConfig(configs[hostname]!, hostname));
  }
}

async function init() {
  if (existsSync(netFilePath)) {
    console.log("Network configuration already exists, not overwriting.");
    return;
  }
  let initialConfig: NetworkConfig = {
      $schema: "./config.schema.json",
      network: {
        interface_name: "awg0",
        awg: generate_awg_params()
      },
      hosts: {
        "example_server": {
          id: 1,
          endpoint: "100.128.128.128",
          port: 63333,
          tags: ["out"]
        },
        "example_host_1": {
          id: 2,
          tags: ["user"]
        }
      },
      properties: [
        {
          action: "masquerade",
          target: "out"
        },
        { "action": "assign-ip", "target": "*", "cidr": "10.0.101.0/24" },
        { "action": "assign-ip", "target": "*", "cidr": "fe80:ca:b0ba::/64" }
      ],
      relations: [
        {
          from: "out",
          to: "user",
          action: "proxy",
          cidr: "0.0.0.0/0"
        },
        {
          from: "out",
          to: "user",
          action: "proxy",
          cidr: "::0/0"
        },
        {
          from: "user",
          to: "out",
          action: "keepalive",
          time: 60
        }
      ],
    }
  writeFileSync(netFilePath, JSON.stringify(initialConfig, null, 2));
}

async function help() {
  console.log("Awgugwa 0.2");
  console.log("---")
  console.log("")
  console.log("./awgugwa.ts init:\n run in an empty directory, then edit the hell out of network_config.json");
  console.log("./awgugwa.ts generate:\n generates config/[host]/[interface].conf files");
  console.log("./awgugwa.ts generate-awg:\n generates and prints new AmneziaWG parameters");
  console.log("")
  console.log("Usage: node awgugwa.ts [generate|init]");
}

if (process.argv.length < 3) {
  await help();
} else {
  if (process.argv[2] == "generate") {
    await generate_configs();
  } else if (process.argv[2] == "init") {
    await init();
  } else if (process.argv[2] == "generate-awg") {
    console.log(JSON.stringify(generate_awg_params()));
  } else {
    await help();
  }
}
