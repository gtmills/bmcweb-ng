# Testing bmcweb-ng with OpenBMC QEMU

This document describes how to boot an OpenBMC `qemuarm` virtual machine and run
a Redfish smoke test suite against it. The test validates that the Redfish API
surface provided by bmcweb-ng matches what the upstream bmcweb delivers.

---

## Quick Start

```bash
# From WSL2 / Linux — full cross-build + inject + test in one command:
bash scripts/run_bmcweb_ng_qemu.sh
```

The script will:

1. Install Rust toolchain + ARM cross-compiler (if not present)
2. Cross-compile bmcweb-ng for `arm-unknown-linux-gnueabihf`
3. Build (or locate pre-built) OpenBMC `qemuarm` image
4. Boot OpenBMC in QEMU with network ports forwarded to localhost
5. Stop upstream bmcweb, inject the bmcweb-ng binary to `/tmp`
6. Start bmcweb-ng inside the VM (plain HTTP on port 80)
7. Run the Redfish smoke test suite against bmcweb-ng
8. Print a pass/fail summary

### Smoke tests only (VM already running)

```bash
# Run the full test suite against a running bmcweb-ng in QEMU:
bash qemu_test_v3.sh
```

---

## Prerequisites

| Tool | Purpose | Auto-installed |
|------|---------|---------------|
| `wget` / `curl` | Downloading images | ✅ (`apt-get`) |
| `jq` | Parsing JSON responses | ✅ |
| `zstd` | Decompressing rootfs images | ✅ |
| `sshpass` | Non-interactive SSH for injection | ✅ |
| `qemu-system-arm` | ARM emulation | `apt-get install qemu-system-arm` |
| OpenBMC qemuarm image | BMC firmware | Built from source (see below) |
| Linux / WSL2 | Required for qemu-system-arm | Manual (WSL2 recommended on Windows) |

### WSL2 Setup (Windows)

```powershell
# In Windows PowerShell (as Administrator) — only needed once:
wsl --install -d Ubuntu
# Restart Windows, then open Ubuntu WSL and continue with bash
```

---

## QEMU Binary

Install `qemu-system-arm` from the Ubuntu `apt` package (version 8.2+ recommended):

```bash
sudo apt-get install qemu-system-arm
```

---

## OpenBMC Image

The OpenBMC image must be either **built from source** or **placed manually**.

### Option A — Build from source (recommended)

```bash
# Runs bitbake inside the OpenBMC tree (~30–60 min, needs ~50 GB free)
BUILD_OPENBMC=1 bash scripts/run_bmcweb_ng_qemu.sh
```

This builds `obmc-phosphor-image` for the `qemuarm` machine and copies the output
images to `target/qemu-test/image/`. Subsequent runs reuse the cached images.

The Yocto build uses `DISTROOVERRIDES += ":df-phosphor-no-webui"` in `local.conf`
to skip the nodejs/webui-vue compile (~40 min). This has no effect on Redfish testing.

### Option B — Place pre-built files manually

Place these files in `target/qemu-test/image/`:

```
target/qemu-test/image/
├── uImage                              ← ARM kernel (uImage format)
├── obmc-phosphor-image-qemuarm.ext4    ← Root filesystem (ext4, uncompressed)
└── qemuarm.dtb                         ← Placeholder (not required for virt machine)
```

If you have the compressed rootfs (`.ext4.zst`), decompress it first:

```bash
zstd -d obmc-phosphor-image-qemuarm*.ext4.zst \
     -o target/qemu-test/image/obmc-phosphor-image-qemuarm.ext4
```

---

## QEMU Command Line

The `qemuarm` machine uses **`-machine virt`** (not `versatilepb`), virtio
block/net devices, and does **not** require a separate DTB file — the device
tree is compiled into the kernel.

```bash
qemu-system-arm \
    -machine    "virt,highmem=off" \
    -cpu        cortex-a15 \
    -m          256 \
    -smp        4 \
    -kernel     uImage \
    -drive      "id=disk0,file=rootfs-rw.ext4,if=none,format=raw" \
    -device     "virtio-blk-device,drive=disk0" \
    -netdev     "user,id=net0,hostfwd=tcp::2443-:443,hostfwd=tcp::2222-:22,hostfwd=tcp::2080-:80" \
    -device     "virtio-net-device,netdev=net0" \
    -device     "virtio-serial-device" \
    -chardev    "null,id=virtcon" \
    -device     "virtconsole,chardev=virtcon" \
    -object     "rng-random,filename=/dev/urandom,id=rng0" \
    -device     "virtio-rng-pci,rng=rng0" \
    -append     "root=/dev/vda rw console=ttyAMA0,115200 ip=dhcp swiotlb=0 ignore_loglevel net.ifnames=0" \
    -display    none \
    -audio      none \
    -serial     "file:qemu.log" \
    -pidfile    "qemu.pid" \
    -daemonize
```

Port forwarding:

| Host port | Guest port | Service |
|-----------|-----------|---------|
| 2443 | 443 | HTTPS (upstream bmcweb) |
| 2222 | 22 | SSH |
| 2080 | 80 | HTTP (bmcweb-ng plain HTTP) |

OpenBMC boots in approximately 25 seconds. bmcweb-ng answers within 3 seconds
of being started.

---

## Smoke Test Suite

The test script (`qemu_test_v3.sh`) runs **56 checks** in four groups:

### Original 17 smoke tests

| Category | Endpoints tested |
|----------|-----------------|
| ServiceRoot | `/redfish/v1` (version, type) |
| Systems | collection + instance |
| Chassis | collection |
| Managers | collection + instance type |
| SessionService | service enabled, POST login → HTTP 201 |
| AccountService | Administrator role |
| TaskService | service enabled |
| UpdateService | service enabled, FirmwareInventory |
| EventService | service enabled |
| NetworkProtocol | type field |
| EthernetInterfaces | type field |
| Auth enforcement | unauthenticated GET → 401 |

### Round 1 — DBus wiring (10 additional)

| Category | What is verified |
|----------|-----------------|
| PowerState | Valid On/Off/Unknown (live DBus or fallback) |
| FirmwareVersion | Field present in Manager response |
| HostName | Present in NetworkProtocol response |
| MACAddress | Present in EthernetInterface/eth0 response |
| Token auth | X-Auth-Token session → GET /Systems/system HTTP 200 |
| LogServices/EventLog | Instance endpoint returns correct type + Entries link |
| Processors collection | Correct @odata.type |
| Memory collection | Correct @odata.type |
| Chassis enumeration | Dynamic DBus enumeration + `/Chassis/chassis` valid |
| 404 enforcement | Bad Processor/Memory/Chassis IDs return 404 |

### Round 2 — Extended DBus (6 additional)

| Category | What is verified |
|----------|-----------------|
| Chassis Power | `/Chassis/chassis/Power` returns correct type |
| Chassis Thermal | `/Chassis/chassis/Thermal` returns correct type |
| Chassis Sensors | `/Chassis/chassis/Sensors` returns correct type |
| BMC reset action | POST `Manager.Reset` returns 204 |
| System reset action | POST `ComputerSystem.Reset` returns 204 |
| NIC enumeration | EthernetInterfaces count ≥ 1 |

### Rounds 3–10 — Full DBus wiring (23 additional)

| Category | What is verified |
|----------|-----------------|
| Boot target | `BootSourceOverrideTarget` is a valid Redfish value or "None" |
| EventLog Entries | Collection type + Members array present |
| PATCH /Systems/system | Sets BootSourceOverrideTarget, returns 200 |
| PATCH NetworkProtocol | Sets HostName, returns 200 |
| Storage collection | `/Systems/system/Storage` returns correct type |
| PATCH EthernetInterface | DHCPv4 patch returns 200 |
| Dynamic NIC validation | GET by live NIC id returns 200 |
| AssetTag | Present on GET /Systems/system |
| PATCH AssetTag | PATCH succeeds and returns updated value |
| Chassis LED | IndicatorLED field present |
| PATCH Chassis LED | PATCH IndicatorLED returns 200 |
| PowerConsumedWatts | Present in Chassis Power response |
| FirmwareInventory DBus | Members list from live software objects |
| CertificateService | `/redfish/v1/CertificateService` returns correct type |
| TelemetryService | `/redfish/v1/TelemetryService` returns correct type |
| Registries stub | `/redfish/v1/Registries` returns correct type |
| JsonSchemas stub | `/redfish/v1/JsonSchemas` returns correct type |
| AccountService lockout | MaxLoginAttemptBeforeLockout field present |
| Create/delete account | POST + DELETE round-trip returns 201 / 200 |
| Metrics endpoint | `/metrics` returns 200 with Prometheus text |
| WebSocket serial | `/console0` upgrade returns 101 |
| Concurrent GETs | 20 simultaneous GETs all return 200 |
| Startup time | bmcweb-ng answers within 3 seconds of start |

---

## Injecting bmcweb-ng Manually

```bash
# 1. Cross-compile bmcweb-ng for ARM
rustup target add arm-unknown-linux-gnueabihf
sudo apt-get install gcc-arm-linux-gnueabihf
cargo build --release --target arm-unknown-linux-gnueabihf

# 2. Stop upstream bmcweb inside the VM
sshpass -p 0penBmc ssh -o StrictHostKeyChecking=no -p 2222 root@localhost \
    "systemctl stop bmcweb.socket bmcweb.service"

# 3. Copy bmcweb-ng binary into /tmp (rootfs is 96% full — /tmp is tmpfs with ~116 MB free)
sshpass -p 0penBmc scp -o StrictHostKeyChecking=no -O -P 2222 \
    target/arm-unknown-linux-gnueabihf/release/bmcwebd-ng \
    root@localhost:/tmp/bmcwebd-ng

# 4. Start bmcweb-ng on port 80 (plain HTTP — no TLS cert needed for testing)
sshpass -p 0penBmc ssh -o StrictHostKeyChecking=no -p 2222 root@localhost \
    "chmod +x /tmp/bmcwebd-ng && RUST_LOG=info nohup /tmp/bmcwebd-ng \
     --config /tmp/bmcweb-config.toml > /tmp/bmcweb-ng.log 2>&1 &"

# 5. Query Redfish via host port 2080 (HTTP)
curl -s -u root:0penBmc http://localhost:2080/redfish/v1 | python3 -m json.tool
```

---

## Debugging

```bash
# Watch the OpenBMC boot log live:
tail -f target/qemu-test/qemu.log

# SSH into the running VM:
sshpass -p 0penBmc ssh -o StrictHostKeyChecking=no -p 2222 root@localhost

# Query bmcweb-ng (plain HTTP, host port 2080):
curl -s -u root:0penBmc http://localhost:2080/redfish/v1 | python3 -m json.tool

# Query upstream bmcweb (HTTPS with self-signed cert, host port 2443):
curl -sk -u root:0penBmc https://localhost:2443/redfish/v1 | python3 -m json.tool

# View bmcweb-ng log inside the VM:
sshpass -p 0penBmc ssh -o StrictHostKeyChecking=no -p 2222 root@localhost \
    "cat /tmp/bmcweb-ng.log"

# Check if bmcweb-ng is running:
sshpass -p 0penBmc ssh -o StrictHostKeyChecking=no -p 2222 root@localhost \
    "ps | grep bmcwebd-ng"

# Stop QEMU manually:
kill $(cat target/qemu-test/qemu.pid)
```

---

## Known Limitations

- **ARM only**: The OpenBMC `qemuarm` platform emulates a 32-bit ARMv7 Cortex-A15.
  bmcweb-ng must be cross-compiled for `arm-unknown-linux-gnueabihf` to run inside
  the VM.

- **Live DBus data**: The `qemuarm` image does run OpenBMC services and bmcweb-ng
  connects to the system DBus. DBus property reads succeed for services that are
  running; handlers fall back gracefully to placeholder values when a property is
  not available (e.g. `FirmwareVersion = "Unknown"` when the Software.Version
  service does not expose the expected object path in QEMU).

- **TLS not configured in test**: The test config sets `tls_cert = ""`, so
  bmcweb-ng runs plain HTTP on port 80 (forwarded to host port 2080). Use
  `curl http://localhost:2080/` when querying bmcweb-ng. Upstream bmcweb still
  answers HTTPS on port 443 (host 2443).

- **Rootfs is 96% full**: The `obmc-phosphor-image` rootfs leaves only ~4 MB free.
  Always inject the binary into `/tmp` (a tmpfs with ~116 MB free) rather than
  `/usr/bin`.

- **`pkill` not available**: OpenBMC's busybox does not include `pkill`. Use
  `kill $(pidof bmcwebd-ng)` or `killall bmcwebd-ng` instead.

- **QEMU startup ~25s**: Emulation is slower than real hardware. On an actual BMC
  SoC (AST2600) startup is expected to be well under 30 seconds total.

- **webui-vue excluded**: The Yocto build uses `DISTROOVERRIDES += ":df-phosphor-no-webui"`
  to skip the nodejs compile. The web UI is not in the test image, which has no
  effect on Redfish API testing.

---

## References

- [OpenBMC run-qemu script](https://github.com/openbmc/openbmc/blob/main/scripts/run-qemu)
- [QEMU virt machine](https://www.qemu.org/docs/master/system/arm/virt.html)
- [Redfish DSP0266 specification](https://www.dmtf.org/standards/redfish)
