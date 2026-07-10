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
3. Download the latest OpenBMC QEMU binary from the Jenkins CI (cached)
4. Download the latest `qemuarm` OpenBMC image from the same CI (cached)
5. Boot OpenBMC in QEMU with network ports forwarded to localhost
6. Run a comprehensive Redfish smoke test against **upstream bmcweb** (baseline)
7. Stop upstream bmcweb, inject the bmcweb-ng binary
8. Start bmcweb-ng inside the VM
9. Run the same smoke tests against **bmcweb-ng**
10. Print a combined pass/fail summary

### Quick Start — smoke tests only (no cross-build)

```bash
# If you only want to run the Redfish smoke suite against an already-running VM:
SKIP_BOOT=1 bash scripts/setup_qemu_test.sh
```

---

## Prerequisites

| Tool | Purpose | Auto-installed |
|------|---------|---------------|
| `wget` / `curl` | Downloading images | ✅ (`apt-get`) |
| `jq` | Parsing Jenkins API JSON | ✅ |
| `zstd` | Decompressing rootfs images | ✅ |
| `sshpass` | Non-interactive SSH for injection | ✅ |
| QEMU ARM binary | Emulation | Downloaded from Jenkins |
| OpenBMC qemuarm image | BMC firmware | Downloaded from Jenkins |
| Linux / WSL2 | Required for qemu-system-arm | Manual (WSL2 recommended on Windows) |

### WSL2 Setup (Windows)

```powershell
# In Windows PowerShell (as Administrator) — only needed once:
wsl --install -d Ubuntu
# Restart Windows, then open Ubuntu WSL and continue with bash
```

---

## QEMU Binary

`qemu-system-arm` is installed automatically from the Ubuntu `apt` package
(`qemu-system-arm` v8.2+). No manual download required.

---

## OpenBMC Image

The OpenBMC Jenkins CI no longer provides publicly accessible artifact
downloads. The image must be either **built from source** or **placed manually**.

### Option A — Build from source (recommended)

```bash
# Runs bitbake inside the OpenBMC tree (~30 min, needs ~50 GB free)
BUILD_OPENBMC=1 bash scripts/run_bmcweb_ng_qemu.sh
```

This clones `openbmc/openbmc` tag `2.18.0` into `target/qemu-test/openbmc-src`,
builds `obmc-phosphor-image` for the `qemuarm` machine, and copies the output
images to `target/qemu-test/image/`. Subsequent runs reuse the cached images.

### Option B — Place pre-built files manually

If you have access to a pre-built OpenBMC qemuarm image (e.g. from a local
Yocto build or a colleague), place these three files in `target/qemu-test/image/`:

```
target/qemu-test/image/
├── uImage                              ← ARM kernel (uImage format)
├── obmc-phosphor-image-qemuarm.ext4    ← Root filesystem (ext4, NOT compressed)
└── qemuarm.dtb                         ← Device tree blob
```

If you have the compressed rootfs (`.ext4.zst`), decompress it first:

```bash
zstd -d obmc-phosphor-image-qemuarm*.ext4.zst -o target/qemu-test/image/obmc-phosphor-image-qemuarm.ext4
```

Then run normally:

```bash
bash scripts/run_bmcweb_ng_qemu.sh
```

---

## QEMU Command Line

The QEMU invocation used by the test script matches the upstream
[`run-qemu`](https://github.com/openbmc/openbmc/blob/main/scripts/run-qemu) script:

```bash
qemu-system-arm \
    -machine   versatilepb \
    -m         256 \
    -drive     "file=rootfs-rw.ext4,if=virtio,format=raw" \
    -net       "nic" \
    -net       "user,hostfwd=tcp::2443-:443,hostfwd=tcp::2222-:22,hostfwd=tcp::2080-:80" \
    -kernel    uImage \
    -dtb       qemuarm.dtb \
    -append    "root=/dev/vda rw console=ttyAMA0,115200" \
    -display   none \
    -serial    "file:qemu.log" \
    -daemonize
```

Port forwarding:

| Host port | Guest port | Service |
|-----------|-----------|---------|
| 2443 | 443 | HTTPS / bmcweb |
| 2222 | 22 | SSH |
| 2080 | 80 | HTTP |

---

## Smoke Test Suite

The test script (`scripts/setup_qemu_test.sh`) runs these checks:

| Category | Endpoints tested |
|----------|-----------------|
| ServiceRoot | `/redfish/v1` (version, type) |
| Systems | collection + instance + type |
| Chassis | collection + instance |
| Managers | collection + instance + type |
| SessionService | service enabled, POST login (201) |
| AccountService | service, accounts, Administrator role |
| TaskService | service enabled |
| UpdateService | service enabled, FirmwareInventory collection |
| EventService | service enabled |
| NetworkProtocol | type field |
| EthernetInterfaces | type field |
| Chassis Power | type field |
| Chassis Thermal | type field |
| Auth enforcement | unauthenticated GET returns 401 |

---

## Injecting bmcweb-ng into the Running VM

`scripts/run_bmcweb_ng_qemu.sh` does all of this automatically. For
manual steps:

```bash
# 1. Cross-compile bmcweb-ng for ARM (requires WSL2/Linux)
rustup target add arm-unknown-linux-gnueabihf
sudo apt-get install gcc-arm-linux-gnueabihf
cargo build --release --target arm-unknown-linux-gnueabihf

# 2. Boot the VM (if not already running)
bash scripts/setup_qemu_test.sh &   # runs QEMU in background via daemonize

# 3. Stop bmcweb inside the VM
sshpass -p 0penBmc ssh -o StrictHostKeyChecking=no -p 2222 root@localhost \
    "systemctl stop bmcweb"

# 4. Copy bmcweb-ng binary into the VM
sshpass -p 0penBmc scp -o StrictHostKeyChecking=no -P 2222 \
    target/arm-unknown-linux-gnueabihf/release/bmcwebd-ng \
    root@localhost:/usr/bin/bmcwebd-ng

# 5. Start bmcweb-ng in place of bmcweb
sshpass -p 0penBmc ssh -o StrictHostKeyChecking=no -p 2222 root@localhost \
    "RUST_LOG=info /usr/bin/bmcwebd-ng --config /etc/bmcweb/config.toml &"

# 6. Re-run the smoke tests (now hitting bmcweb-ng)
SKIP_BOOT=1 bash scripts/setup_qemu_test.sh
```

---

## Debugging

```bash
# Watch the OpenBMC boot log live:
tail -f target/qemu-test/qemu.log

# SSH into the running VM:
ssh -o StrictHostKeyChecking=no -p 2222 root@localhost
# password: 0penBmc

# Query Redfish directly:
curl -sk -u root:0penBmc https://localhost:2443/redfish/v1 | jq .

# Check bmcweb status inside the VM:
ssh -p 2222 root@localhost "systemctl status bmcweb"
ssh -p 2222 root@localhost "journalctl -u bmcweb -f"

# Stop QEMU manually:
kill $(cat target/qemu-test/qemu.pid)
```

---

## Known Limitations

- **ARM only**: The OpenBMC `qemuarm` platform emulates a 32-bit ARMv7 Cortex-A9.
  bmcweb-ng must be cross-compiled for `arm-unknown-linux-gnueabihf` to run inside
  the VM. Native x86-64 testing exercises the Rust logic but not the ARM ABI.

- **No DBus services**: The bare QEMU image does not run the full set of OpenBMC
  DBus services. DBus-backed responses (power state, sensor values, firmware
  version, etc.) will return static placeholder values until those services are
  present.

- **Self-signed TLS**: The OpenBMC QEMU image uses a self-signed certificate.
  All `curl` commands use `-k` (insecure) to skip certificate verification.
  In production, replace with a proper certificate or configure CA trust.

---

## References

- [OpenBMC run-qemu script](https://github.com/openbmc/openbmc/blob/main/scripts/run-qemu)
- [OpenBMC CI Jenkins](https://jenkins.openbmc.org/)
- [QEMU versatilepb machine](https://www.qemu.org/docs/master/system/arm/versatile.html)
- [Redfish DSP0266 specification](https://www.dmtf.org/standards/redfish)
