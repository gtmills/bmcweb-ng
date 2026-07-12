# Testing bmcweb-ng with OpenBMC QEMU

This document describes how to boot OpenBMC virtual machines and run a Redfish
smoke test suite against bmcweb-ng.  Two machine targets are supported:

| Target | Machine model | Script | Notes |
|--------|--------------|--------|-------|
| Generic `qemuarm` | `virt` (Cortex-A15) | `run_bmcweb_ng_qemu.sh` | Original upstream OpenBMC |
| **p10bmc / IBM Rainier** | `rainier-bmc` (AST2600) | `run_rainier_qemu.sh` | IBM fork, WIC qcow2 image |

Jump to:
- [Generic qemuarm (original)](#generic-qemuarm-original)
- [p10bmc / IBM Rainier](#p10bmc--ibm-rainier-qemu)

---

## Generic qemuarm (original)

### Quick Start

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

The test script (`qemu_test_v3.sh`) runs **63 checks**:

| # | Endpoint / Action | What is verified |
|---|-------------------|-----------------|
| 1 | `GET /redfish/v1` | `@odata.type` = ServiceRoot v1_15_0 |
| 2 | `GET /redfish/v1` | `RedfishVersion` = 1.17.0 |
| 3 | `GET /redfish/v1/Systems` | Correct collection type |
| 4 | `GET /redfish/v1/Systems/system` | `Id` = system |
| 5 | `GET /redfish/v1/Chassis` | Correct collection type |
| 6 | `GET /redfish/v1/Managers` | Correct collection type |
| 7 | `GET /redfish/v1/Managers/bmc` | `ManagerType` = BMC |
| 8 | `GET /redfish/v1/SessionService` | `ServiceEnabled` = True |
| 9 | `POST /redfish/v1/SessionService/Sessions` | HTTP 201 |
| 10 | `GET /redfish/v1/AccountService/Roles/Administrator` | `IsPredefined` = True |
| 11 | `GET /redfish/v1/TaskService` | `ServiceEnabled` = True |
| 12 | `GET /redfish/v1/UpdateService` | `ServiceEnabled` = True |
| 13 | `GET /redfish/v1/UpdateService/FirmwareInventory` | Correct collection type |
| 14 | `GET /redfish/v1/EventService` | `ServiceEnabled` = True |
| 15 | `GET /redfish/v1/Managers/bmc/NetworkProtocol` | Correct resource type |
| 16 | `GET /redfish/v1/Managers/bmc/EthernetInterfaces` | Correct collection type |
| 17 | Unauthenticated `GET /redfish/v1/Systems` | HTTP 401 |
| 18 | `GET /redfish/v1/Systems/system` | `PowerState` is On, Off, or Unknown |
| 19 | `GET /redfish/v1/Managers/bmc` | `FirmwareVersion` field present |
| 20 | `GET /redfish/v1/Managers/bmc/NetworkProtocol` | `HostName` field present |
| 21 | `GET /redfish/v1/Managers/bmc/EthernetInterfaces/eth0` | HTTP 200 |
| 22 | `GET /redfish/v1/Managers/bmc/EthernetInterfaces/eth0` | `MACAddress` field present |
| 23 | X-Auth-Token session login then `GET /Systems/system` | HTTP 200 |
| 24 | `GET /redfish/v1/Systems/system/LogServices/EventLog` | Correct resource type |
| 25 | `GET /redfish/v1/Systems/system/LogServices/EventLog` | `Id` = EventLog |
| 26 | `GET /redfish/v1/Systems/system/LogServices/EventLog` | `Entries` link present |
| 27 | `GET /redfish/v1/Systems/system/Processors` | Correct collection type |
| 28 | `GET /redfish/v1/Systems/system/Memory` | Correct collection type |
| 29 | `GET /redfish/v1/Chassis` | Correct collection type (dynamic enumeration) |
| 30 | `GET /redfish/v1/Chassis/chassis` | `Id` = chassis |
| 31 | `GET /redfish/v1/Systems/system/Processors/nonexistent999` | HTTP 404 |
| 32 | `GET /redfish/v1/Systems/system/Memory/nonexistent999` | HTTP 404 |
| 33 | `GET /redfish/v1/Chassis/badchassis999` | HTTP 404 |
| 34 | `GET /redfish/v1/Systems/system` | `Boot.BootSourceOverrideTarget` is a valid Redfish value |
| 35 | `GET /redfish/v1/Systems/system/LogServices/EventLog/Entries` | Correct collection type |
| 36 | `GET /redfish/v1/Systems/system/LogServices/EventLog/Entries` | `Members` array present |
| 37 | `PATCH /redfish/v1/Systems/system` | Boot override (Pxe/Once) returns 200 |
| 38 | `PATCH /redfish/v1/Managers/bmc/NetworkProtocol` | HostName + NTPServers update returns 200 |
| 39 | `GET /redfish/v1/CertificateService` | `Id` = CertificateService |
| 40 | `GET /redfish/v1/TelemetryService` | `Id` = TelemetryService |
| 41 | `GET /redfish/v1/TelemetryService/MetricDefinitions` | Correct collection type |
| 42 | `GET /redfish/v1/Managers/bmc/LogServices/BMC` | `Id` = BMC |
| 43 | `GET /redfish/v1/Managers/bmc/LogServices/BMC/Entries` | Correct collection type |
| 44 | `GET /redfish/v1/Systems/system/NetworkInterfaces` | Correct collection type |
| 45 | `GET /redfish/v1/AccountService` | `Id` = AccountService |
| 46 | `GET /redfish/v1/AccountService/PrivilegeMap` | `Id` = PrivilegeMap |
| 47 | `GET /redfish/v1/Registries` | Correct collection type |
| 48 | `GET /redfish/v1/JsonSchemas` | Correct collection type |
| 49 | `GET /redfish/v1/CertificateService/CertificateLocations` | `Id` = CertificateLocations |
| 50 | `PATCH /redfish/v1/AccountService` | Lockout threshold update returns 200 |
| 51 | `GET /redfish/v1/UpdateService/FirmwareInventory` | Correct collection type (DBus enriched) |
| 52 | `GET /redfish/v1/Systems/system` | `AssetTag` field present |
| 53 | `PATCH /redfish/v1/Systems/system` | AssetTag update returns 200 |
| 54 | `GET /redfish/v1/Chassis/chassis` | `IndicatorLED` field present |
| 55 | `PATCH /redfish/v1/Chassis/chassis` | IndicatorLED update returns 200 |
| 56 | `GET /redfish/v1/Chassis/chassis/Power` | `PowerControl` array present |
| 57 | `GET /health` | `status` is ok or degraded (JSON health endpoint) |
| 58 | `GET /health` | `version` field present |
| 59 | `PATCH /redfish/v1/SessionService` | `SessionTimeout` update returns 200 |
| 60 | `GET /redfish/v1/SessionService` | `SessionTimeout` reflects patched value |
| 61 | `PATCH /redfish/v1/EventService` | `DeliveryRetryAttempts` update returns 200 |
| 62 | `GET /redfish/v1/EventService` | `DeliveryRetryAttempts` reflects patched value |
| 63 | `GET /redfish/v1/Chassis/chassis/NetworkAdapters` | Correct collection type |

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

## p10bmc / IBM Rainier QEMU

The IBM Rainier system uses an AST2600 BMC.  QEMU models this with the
`rainier-bmc` machine type (added in QEMU 7.1.0).  The image layout is
significantly different from generic `qemuarm`:

| Aspect | qemuarm | Rainier |
|--------|---------|---------|
| QEMU machine | `virt,highmem=off` | `rainier-bmc,boot-emmc=false` |
| Kernel format | `uImage` | `fitImage-linux.bin` (FIT) |
| DTB | compiled into kernel | `aspeed-bmc-ibm-rainier.dtb` (separate) |
| Initramfs | not used | `obmc-phosphor-initramfs.rootfs.cpio.xz` |
| Root filesystem | raw `ext4` | `wic.qcow2` (partitioned, `PARTLABEL=rofs-a`) |
| Block device arg | `-drive if=virtio` | `-drive if=sd,index=2` |
| OpenBMC source | upstream `openbmc/openbmc` | IBM fork `ibm-openbmc/openbmc` |
| Boot time (QEMU) | ~25s | ~60–90s |

### Quick Start

> **A p10bmc image must be built first.**  The generic upstream OpenBMC
> `qemuarm` image will not work — it has the wrong kernel, DTB, and filesystem
> layout for the `rainier-bmc` machine model.

**First time** (builds the p10bmc image from source, ~60 min, ~80 GB):

```bash
BUILD_P10BMC=1 bash scripts/run_rainier_qemu.sh
```

**Subsequent runs** (image cached in `target/qemu-test/rainier-image/`):

```bash
bash scripts/run_rainier_qemu.sh
```

**If you already have a p10bmc Yocto deploy directory:**

```bash
IMAGEPATH=/path/to/tmp/deploy/images/rainier bash scripts/run_rainier_qemu.sh
```

The script performs the same 11-step flow as the qemuarm script but uses the
`rainier-bmc` machine model, qcow2 overlay, and the IBM OpenBMC fork.

### Prerequisites

Same as qemuarm, plus:

- **QEMU ≥ 7.1.0** — `rainier-bmc` machine was added in 7.1.  Check with
  `qemu-system-arm -machine help | grep rainier`.  The Ubuntu 22.04 apt
  package (`qemu-system-arm 6.2`) is **too old**.  Install the OpenBMC
  Jenkins binary or build QEMU from source:

  ```bash
  # Download the OpenBMC-built QEMU binary (x86_64, statically linked)
  wget https://jenkins.openbmc.org/job/latest-qemu-x86/lastSuccessfulBuild/artifact/qemu/build/qemu-system-arm
  chmod +x qemu-system-arm
  sudo mv qemu-system-arm /usr/local/bin/
  sudo apt-get install -y libfdt1   # runtime dep for Jenkins binary
  ```

- **`qemu-utils`** — needed for `qemu-img create` (qcow2 overlay).
  Auto-installed by the script via `apt_install qemu-utils`.

### Image Files

Four files are required in `target/qemu-test/rainier-image/`
(or override with `IMAGEPATH=/path/to/dir`):

```
fitImage-linux.bin                        ← FIT image (kernel + built-in DTB)
aspeed-bmc-ibm-rainier.dtb                ← Rainier BMC device tree blob
obmc-phosphor-initramfs.rootfs.cpio.xz    ← initramfs (mounts squashfs overlay)
obmc-phosphor-image.rootfs.wic.qcow2      ← SD card WIC image (contains rofs-a)
```

**Option A — Build from source** (recommended, ~60 min, ~80 GB):

```bash
BUILD_P10BMC=1 bash scripts/run_rainier_qemu.sh
```

This clones `ibm-openbmc/openbmc`, runs `bitbake obmc-phosphor-image` for the
`rainier` machine, and copies the four files to `target/qemu-test/rainier-image/`.
The build must happen on the WSL2 ext4 VHD (`~/...`), **not** on `/mnt/c/` —
BitBake uses UNIX domain sockets that are not supported on NTFS.

**Option B — Use a pre-built deploy directory**:

```bash
IMAGEPATH=/path/to/tmp/deploy/images/rainier bash scripts/run_rainier_qemu.sh
```

### QEMU Command Line

The exact command issued by `run_rainier_qemu.sh`:

```bash
qemu-system-arm \
    -M "rainier-bmc,boot-emmc=false" \
    -nographic \
    -kernel  fitImage-linux.bin \
    -dtb     aspeed-bmc-ibm-rainier.dtb \
    -initrd  obmc-phosphor-initramfs.rootfs.cpio.xz \
    -drive   "file=rainier-rw.qcow2,if=sd,index=2,format=qcow2" \
    -netdev  "user,id=net0,hostfwd=tcp::2443-:443,hostfwd=tcp::2222-:22,hostfwd=tcp::2080-:80" \
    -net     "nic,netdev=net0" \
    -append  "console=ttyS4,115200n8 rootwait root=PARTLABEL=rofs-a" \
    -serial  "file:rainier-qemu.log" \
    -pidfile "rainier-qemu.pid" \
    -daemonize
```

The script passes a **qcow2 overlay** (`rainier-rw.qcow2`) backed by the
pristine WIC image so the base image is never written to.  The overlay is
deleted on exit.

Port forwarding:

| Host port | Guest port | Service |
|-----------|-----------|---------|
| 2443 | 443 | HTTPS (upstream bmcweb / bmcweb-ng TLS) |
| 2222 | 22  | SSH |
| 2080 | 80  | HTTP (bmcweb-ng plain HTTP for testing) |

### bmcweb-ng Injection

The p10bmc rootfs uses a read-only squashfs (`rofs-a`).  The binary is always
injected into `/tmp` (a tmpfs) — never the rootfs:

```bash
# Cross-compile (once):
cargo build --release --target arm-unknown-linux-gnueabihf

# Stop upstream bmcweb:
sshpass -p 0penBmc ssh -p 2222 root@localhost \
    "systemctl stop bmcweb.socket bmcweb.service"

# Copy binary to /tmp (tmpfs — not rofs-a):
sshpass -p 0penBmc scp -O -P 2222 \
    target/arm-unknown-linux-gnueabihf/release/bmcwebd-ng \
    root@localhost:/tmp/bmcwebd-ng

# Start bmcweb-ng on port 80 (plain HTTP):
sshpass -p 0penBmc ssh -p 2222 root@localhost \
    "chmod +x /tmp/bmcwebd-ng && RUST_LOG=info \
     nohup /tmp/bmcwebd-ng --config /tmp/config.toml \
     > /tmp/bmcweb-ng.log 2>&1 &"

# Query via host port 2080:
curl -s -u root:0penBmc http://localhost:2080/redfish/v1 | python3 -m json.tool
```

### Debugging

```bash
# Watch the Rainier boot log:
tail -f target/qemu-test/rainier-qemu.log

# SSH into the VM:
sshpass -p 0penBmc ssh -p 2222 root@localhost

# Query bmcweb-ng (plain HTTP):
curl -s -u root:0penBmc http://localhost:2080/redfish/v1 | python3 -m json.tool

# Query upstream bmcweb (HTTPS):
curl -sk -u root:0penBmc https://localhost:2443/redfish/v1 | python3 -m json.tool

# View bmcweb-ng log inside VM:
sshpass -p 0penBmc ssh -p 2222 root@localhost "cat /tmp/bmcweb-ng.log"

# Check if bmcweb-ng is running:
sshpass -p 0penBmc ssh -p 2222 root@localhost "ps | grep bmcwebd-ng"

# Stop QEMU:
kill $(cat target/qemu-test/rainier-qemu.pid)
```

### Known Limitations (Rainier)

- **QEMU ≥ 7.1 required**: The `rainier-bmc` machine type is not in the
  Ubuntu 22.04 apt package.  Use the OpenBMC Jenkins binary or build from source.

- **Slow boot (~60–90s)**: The Rainier machine initialises more phosphor
  services than `qemuarm`.  The script polls for up to 10 minutes.

- **Read-only rootfs**: `rofs-a` is a squashfs partition.  All writable state
  (including the injected binary) must go to `/tmp` or `/var`.

- **No eMMC in QEMU**: The `-M rainier-bmc,boot-emmc=false` flag is required.
  Without it QEMU looks for an eMMC device that is not wired up, and the
  machine fails to boot.

- **`-nographic` required**: The Rainier machine model does not support the
  `-display none` flag used by the qemuarm script.  Use `-nographic` instead;
  combined with `-serial file:...` and `-daemonize` this is equivalent.

- **TLS for testing**: bmcweb-ng is started on plain HTTP (:80) for testing so
  no certificate management is needed.  Upstream bmcweb still answers HTTPS on
  :443 (host 2443).

- **DBus fallbacks**: Some OpenBMC services (e.g. `phosphor-virtual-sensor`,
  IBM-specific inventory) may not be fully running in the QEMU environment.
  bmcweb-ng falls back gracefully to placeholder values for those fields.

---

## References

- [OpenBMC run-qemu script](https://github.com/openbmc/openbmc/blob/main/scripts/run-qemu)
- [IBM OpenBMC fork](https://github.com/ibm-openbmc/openbmc)
- [QEMU virt machine](https://www.qemu.org/docs/master/system/arm/virt.html)
- [QEMU rainier-bmc machine](https://www.qemu.org/docs/master/system/arm/aspeed.html)
- [Redfish DSP0266 specification](https://www.dmtf.org/standards/redfish)
