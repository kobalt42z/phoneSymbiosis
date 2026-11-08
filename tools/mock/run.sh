#!/bin/bash
# Runs inside dbus-run-session (session bus) with a private "system" bus.
D=$(cd "$(dirname "$0")" && pwd)

dbus-daemon --config-file=$D/sysbus.conf --fork --print-pid > $D/sys.pid
sleep 0.3
export DBUS_SYSTEM_BUS_ADDRESS=unix:abstract=phonesym-mock-sys
export RUST_BACKTRACE=0
P=${PHONESYM:-$D/../../target/debug/phonesym}
${PYTHON:-python3} $D/mock_tel.py 2500 > $D/mock.log 2>&1 &
MOCK=$!
sleep 1
echo "== device list"; $P device list
echo "== device info"; $P --json device info a4:83:e7:12:34:56
echo "== pair"; echo y | $P device pair A4:83:E7:12:34:56
echo "== watch (bg)"; timeout 6 $P watch > $D/watch.out 2>&1 &
echo "== call list"; $P call list
echo "== dial"; $P call dial "+1 (555) 000-1111"
sleep 0.3
echo "== call list json"; $P --json call list
echo "== volume"; $P call volume; $P call volume --speaker 5; 
echo "== dtmf"; $P call dtmf "12#"
echo "== status"; $P call status
sleep 1.6
echo "== list after ring"; $P call list
echo "== answer"; $P call answer
$P call list
echo "== hangup all"; $P call hangup
sleep 0.5
$P call list
echo "== doctor"; $P doctor
sleep 2.5
echo "== watch output"; cat $D/watch.out
kill $MOCK; kill $(cat $D/sys.pid)
echo "== mock log"; cat $D/mock.log
