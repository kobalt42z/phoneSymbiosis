# Fake phone for testing without hardware

Python D-Bus mocks of PipeWire telephony and BlueZ, plus fake `pw-dump` /
`pw-loopback`, so the CLI can be exercised end to end in a container.

```sh
cargo build -p phonesym-cli
sudo apt install python3-dbus python3-gi dbus   # mock dependencies
dbus-run-session -- tools/mock/run.sh    # devices, pairing, calls, volume, doctor
dbus-run-session -- tools/mock/run2.sh   # automatic call audio routing
```
