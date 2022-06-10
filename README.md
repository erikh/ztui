# ztui: your one-stop shop for ZeroTier terminal UI goodness

ztui aims to be a frontend for all interactions with ZeroTier in an attempt to
transform how people work with it. It provides the following features:

- Main Screen:
  - Bookmarks for Networks (disconnecting does not make the network id disappear from the list, and you can rejoin easily)
  - Interaction directly with Central members from the network list.
  - Joining, Leaving Networks
  - Per-Network bandwidth statistics
  - Bind arbitrary commands to keys that use a template to launch (see more on this below)
  - Review the network JSON formatted pretty
  - Launch `$EDITOR` against a file of network rules (and save them back to central)
- Central / Member List:
  - Rename members
  - Auth, Unauth, and Delete members

Networks List View:

<center><img src="readme-images/main.png" /></center>

Members List / Network View:

<center><img src="readme-images/network.png" /></center>

## Installing

Get [Rust](https://www.rustup.rs) 1.60 or better if you need to.

```
cargo install --git https://github.com/erikh/ztui
```

## Configuring arbitrary commands

### Rules

- Command must not be mapped by existing commands
- Will be executed in a shell; quote accordingly

### Configuration Syntax

After you start `ztui` for the first time, `$HOME/.config.zerotier/settings.json` will be created for you with your last-saved network information. Now, what we want to do is create `$HOME/.config.zerotier/config.json` and add something like this:

```json
{
        "network_commands": {
                "1": "/bin/tcpdump -i %i"
        },
        "member_commands": {
                "1": "/bin/iperf -c %a"
        }
}
```

Network format strings available:

- `%i`: the interface of the ZeroTier network
- `%n`: the network ID of the ZeroTier network
- `%a`: the first addresses in the list of assigned IP addresses

In this case, it would allow me to press `1` over a network to `tcpdump` its interface; then I would control+C out of it to come back to `ztui`.

Member format strings available:

- `%n`: the network ID of the ZeroTier network
- `%i`: the identity of the member of this ZeroTier network
- `%a`: the first assigned IP address of this member
- `%N`: the name (not the fqdn!) of the ZeroTier network member as it appears in central

In the above example, it allows me to start an `iperf` client against the address of the selected member.

## Author

Erik Hollensbe <git@hollensbe.org>
