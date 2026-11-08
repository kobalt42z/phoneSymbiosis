#!/bin/bash
D=$(cd "$(dirname "$0")" && pwd)
dbus-daemon --config-file=$D/sysbus.conf --fork --print-pid > $D/sys.pid
sleep 0.3
export DBUS_SYSTEM_BUS_ADDRESS=unix:abstract=phonesym-mock-sys RUST_BACKTRACE=0 PATH=$D/bin:$PATH
P=${PHONESYM:-$D/../../target/debug/phonesym}
${PYTHON:-python3} $D/mock_tel.py 1000 > $D/mock2.log 2>&1 &
MOCK=$!
sleep 0.5
timeout -s INT 5 $P watch --audio > $D/watch2.out 2>&1 &
sleep 1.5
echo "== reject pairing"; echo n | $P device pair A4:83:E7:12:34:56
echo "== answer"; $P call answer
sleep 1.5
echo "== loopbacks running:"; pgrep -af 'pw-loopback --name' | sed 's/^[0-9]* //'
$P call hangup
sleep 1.5
echo "== loopbacks after hangup:"; pgrep -af 'pw-loopback --name' | wc -l
echo "== audio on/off in separate processes"
$P call audio on & AON=$!
sleep 7
pgrep -af 'pw-loopback --name' | wc -l
$P call audio off
sleep 0.3; pgrep -af 'pw-loopback --name' | wc -l
kill $AON 2>/dev/null
cat $D/watch2.out; cat $D/loopback.log
kill $MOCK; kill $(cat $D/sys.pid)
grep -v RegisterAgent $D/mock2.log
