"""Mock of PipeWire's org.pipewire.Telephony (session bus) and BlueZ (system bus)."""
import sys, dbus, dbus.service, dbus.mainloop.glib
from gi.repository import GLib

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
PROPS = "org.freedesktop.DBus.Properties"
ADDR = "A4:83:E7:12:34:56"
LOG = []

def log(*a):
    print("MOCK:", *a, flush=True)

class Call(dbus.service.Object):
    def __init__(self, bus, path, state, number):
        super().__init__(bus, path)
        self.path = path
        self.p = {"LineIdentification": number, "IncomingLine": "", "Name": "", "Multiparty": dbus.Boolean(False), "State": state}
    @dbus.service.method("org.pipewire.Telephony.Call1")
    def Answer(self):
        log("Answer", self.path)
        self.set_state("active")
    @dbus.service.method("org.pipewire.Telephony.Call1")
    def Hangup(self):
        log("Hangup", self.path)
        self.set_state("disconnected")
        GLib.timeout_add(50, lambda: ag.remove_call(self) or False)
    def set_state(self, s):
        self.p["State"] = s
        self.PropertiesChanged("org.pipewire.Telephony.Call1", {"State": s}, [])
        self.PropertyChanged("State", s)
    @dbus.service.signal(PROPS, signature="sa{sv}as")
    def PropertiesChanged(self, i, c, inv): pass
    @dbus.service.signal("org.ofono.VoiceCall", signature="sv")
    def PropertyChanged(self, n, v): pass
    @dbus.service.method(PROPS, in_signature="s", out_signature="a{sv}")
    def GetAll(self, i): return self.p

class AG(dbus.service.Object):
    def __init__(self, bus, path):
        super().__init__(bus, path)
        self.bus = bus; self.path = path; self.calls = []; self.n = 0
        self.vol = {"SpeakerVolume": dbus.Byte(10), "MicrophoneVolume": dbus.Byte(12)}
    def add_call(self, state, number):
        c = Call(self.bus, f"{self.path}/call{self.n}", state, number); self.n += 1
        self.calls.append(c)
        self.CallAdded(c.path, c.p)
        return c
    def remove_call(self, c):
        self.calls.remove(c); c.remove_from_connection()
        self.CallRemoved(c.path)
    @dbus.service.method("org.pipewire.Telephony.AudioGateway1", in_signature="s")
    def Dial(self, n):
        log("Dial", n)
        c = self.add_call("dialing", n)
        GLib.timeout_add(100, lambda: c.set_state("alerting") or False)
    @dbus.service.method("org.pipewire.Telephony.AudioGateway1")
    def HangupAll(self):
        log("HangupAll")
        for c in list(self.calls): c.Hangup()
    @dbus.service.method("org.pipewire.Telephony.AudioGateway1", in_signature="s")
    def SendTones(self, t): log("SendTones", t)
    @dbus.service.method("org.pipewire.Telephony.AudioGateway1")
    def SwapCalls(self): log("SwapCalls")
    @dbus.service.method("org.pipewire.Telephony.AudioGateway1")
    def CreateMultiparty(self): log("CreateMultiparty")
    @dbus.service.method("org.ofono.VoiceCallManager", out_signature="a(oa{sv})")
    def GetCalls(self): return [(c.path, c.p) for c in self.calls]
    @dbus.service.signal("org.ofono.VoiceCallManager", signature="oa{sv}")
    def CallAdded(self, p, props): pass
    @dbus.service.signal("org.ofono.VoiceCallManager", signature="o")
    def CallRemoved(self, p): pass
    @dbus.service.method(PROPS, in_signature="ss", out_signature="v")
    def Get(self, i, n):
        if i == "org.pipewire.Telephony.AudioGateway1" and n == "Address": return ADDR
        if i == "org.pipewire.Telephony.AudioGatewayTransport1" and n == "State": return "idle"
        if n in self.vol: return self.vol[n]
        raise dbus.exceptions.DBusException("no such prop", name="org.freedesktop.DBus.Error.InvalidArgs")
    @dbus.service.method(PROPS, in_signature="s", out_signature="a{sv}")
    def GetAll(self, i):
        d = dict(self.vol); d["Address"] = ADDR; return d
    @dbus.service.method(PROPS, in_signature="ssv")
    def Set(self, i, n, v):
        log("Set", i, n, int(v)); self.vol[n] = dbus.Byte(int(v))
        self.PropertiesChanged(i, {n: dbus.Byte(int(v))}, [])
    @dbus.service.signal(PROPS, signature="sa{sv}as")
    def PropertiesChanged(self, i, c, inv): pass
    @dbus.service.method("org.pipewire.Telephony.AudioGatewayTransport1")
    def Activate(self): log("Activate")

class Manager(dbus.service.Object):
    @dbus.service.method("org.ofono.Manager", out_signature="a(oa{sv})")
    def GetModems(self):
        return [(ag.path, {"Interfaces": dbus.Array(["org.ofono.VoiceCallManager"], signature="s")})]

# ---- BlueZ mock on the system bus ----
class Root(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, "/")
        self.paired = False; self.connected = False; self.trusted = False
    def dev_props(self):
        return {"Address": ADDR, "Alias": "Test iPhone", "Paired": dbus.Boolean(self.paired), "Trusted": dbus.Boolean(self.trusted),
                "Connected": dbus.Boolean(self.connected), "RSSI": dbus.Int16(-55),
                "UUIDs": dbus.Array(["0000111f-0000-1000-8000-00805f9b34fb", "00001132-0000-1000-8000-00805f9b34fb"], signature="s")}
    @dbus.service.method("org.freedesktop.DBus.ObjectManager", out_signature="a{oa{sa{sv}}}")
    def GetManagedObjects(self):
        return {"/org/bluez": {"org.bluez.AgentManager1": {}},
                "/org/bluez/hci0": {"org.bluez.Adapter1": {"Address": "00:1A:7D:DA:71:13", "Alias": "pc", "Powered": dbus.Boolean(True), "Discoverable": dbus.Boolean(False)}},
                "/org/bluez/hci0/dev_A4_83_E7_12_34_56": {"org.bluez.Device1": self.dev_props()}}

class AgentMgr(dbus.service.Object):
    agent = None
    @dbus.service.method("org.bluez.AgentManager1", in_signature="os", sender_keyword="sender")
    def RegisterAgent(self, path, cap, sender=None):
        log("RegisterAgent", path, cap); AgentMgr.agent = (sender, path)
    @dbus.service.method("org.bluez.AgentManager1", in_signature="o")
    def RequestDefaultAgent(self, path):
        raise dbus.exceptions.DBusException("already", name="org.bluez.Error.AlreadyExists")

class Device(dbus.service.Object):
    @dbus.service.method("org.bluez.Device1", async_callbacks=("ok", "err"))
    def Pair(self, ok, err):
        sender, path = AgentMgr.agent
        agent = sysbus.get_object(sender, path)
        def reply(*a):
            log("agent accepted"); root.paired = True; ok()
        def error(e):
            log("agent rejected", e.get_dbus_name()); err(dbus.exceptions.DBusException("rej", name="org.bluez.Error.AuthenticationRejected"))
        agent.RequestConfirmation(self.__dbus_object_path__, dbus.UInt32(123456), dbus_interface="org.bluez.Agent1",
                                  reply_handler=reply, error_handler=error, timeout=60)
    @dbus.service.method("org.bluez.Device1")
    def Connect(self):
        log("Connect"); root.connected = True
        self.PropertiesChanged("org.bluez.Device1", {"Connected": dbus.Boolean(True)}, [])
    @dbus.service.method("org.bluez.Device1")
    def Disconnect(self):
        log("Disconnect"); root.connected = False
        self.PropertiesChanged("org.bluez.Device1", {"Connected": dbus.Boolean(False)}, [])
    @dbus.service.signal(PROPS, signature="sa{sv}as")
    def PropertiesChanged(self, i, c, inv): pass
    @dbus.service.method(PROPS, in_signature="ssv")
    def Set(self, i, n, v):
        log("Device Set", n, bool(v)); root.trusted = bool(v)
    @dbus.service.method(PROPS, in_signature="s", out_signature="a{sv}")
    def GetAll(self, i): return root.dev_props()

sess = dbus.SessionBus()
name = dbus.service.BusName("org.pipewire.Telephony", sess)
mgr = Manager(sess, "/org/pipewire/Telephony")
ag = AG(sess, "/org/pipewire/Telephony/ag0")

sysbus = dbus.SystemBus()
bname = dbus.service.BusName("org.bluez", sysbus)
root = Root(sysbus)
am = AgentMgr(sysbus, "/org/bluez")
dev = Device(sysbus, "/org/bluez/hci0/dev_A4_83_E7_12_34_56")

def ring():
    log("ringing"); ag.add_call("incoming", "+15551234567"); return False
GLib.timeout_add(int(sys.argv[1]) if len(sys.argv) > 1 else 1500, ring)
log("ready")
GLib.MainLoop().run()
