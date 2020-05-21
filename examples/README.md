# Running these examples

All of the examples provided here work on top of a virtual TAP interface provided
by the Linux kernel Before running the examples, you need to create a TAP interface
with Internet access usable by non-privileged users.

To spawn a TAP interface named `tap0`, run the following commands:

```no_rust
sudo ip tuntap add name tap0 mode tap user $USER
sudo ip link set tap0 up
sudo ip addr add 192.168.69.100/24 dev tap0
```

To forward IPv4 traffic to/from the interface, run:

```no_rust
sudo iptables -t nat -A POSTROUTING -s 192.168.69.0/24 -j MASQUERADE
sudo sysctl net.ipv4.ip_forward=1
```

Adjust the interface IP appropriately if you happen to already be on a 192.168.69.0/24 network.
If you do, remember to adjust the example accordingly.
