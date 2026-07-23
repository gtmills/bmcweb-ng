# Changelog

All notable changes to bmcweb-ng will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

---

## [0.4.1] - 2026-07-15

### Added

- **10 new upstream Redfish endpoints (Round 4 upstream sync)**

  - `GET /redfish/v1/odata` — OData service document listing all top-level Redfish
    collections as JSON; `$metadata` CSDL XML already served from `http.rs`
    (`redfish-core/lib/odata.hpp`)

  - `GET /Fabrics[/{id}/Switches[/{id}]]` — Fabric collection + Fabric instance +
    Switches collection + Switch instance; enumerates `Inventory.Item.PCIeSwitch`
    DBus objects (`redfish-core/lib/fabric.hpp`)

  - `GET /Chassis/{id}/NetworkAdapters/{id}` — NetworkAdapter instance; reads
    Manufacturer, Model, PartNumber, SerialNumber from `Decorator.Asset` DBus
    (`redfish-core/lib/network_adapter.hpp`)

  - `GET /Systems/{id}/Storage/{id}/Controllers/{id}` — StorageController instance;
    reads asset data and `Present` flag; synthesised controller "0" for `Storage/1`
    (`redfish-core/lib/storage_controller.hpp`)

  - `GET /Systems/{id}/Processors/{id}/OperatingConfigs[/{id}]` — OperatingConfig
    collection and instance; reads BaseSpeed, MaxSpeed, TDPWatts, MaxJunctionTemp,
    AvailableCoreCount from `Inventory.Item.Cpu.OperatingConfig` DBus
    (`redfish-core/lib/processor_operating_config.hpp`)

  - `GET /Managers/{id}/LogServices/DBusEventLog[/Entries]` — Manager DBus event log
    service backed by `xyz.openbmc_project.Logging`; Manager LogServices count
    updated 3 → 4 (`redfish-core/lib/manager_logservices_dbus_eventlog.hpp`)

  - `GET /Chassis/{id}/Drives[/{id}]` — Drive collection and instance; maps
    `DriveType` (HDD/SSD) and `Protocol` (NVMe/SATA/SAS) from DBus enum strings
    (`redfish-core/lib/storage_chassis.hpp`)

  - `GET /Systems/{id}/FabricAdapters[/{id}]` — FabricAdapter collection and instance;
    reads Manufacturer, Model, PartNumber, LocationCode from DBus
    (`redfish-core/lib/fabric_adapters.hpp`)

### Fixed

- **IndicatorLED Blinking state** (`chassis.rs`) — `GET /Chassis/{id}` now checks
  `enclosure_identify_blink` group first (→ `"Blinking"`), then `enclosure_identify`
  (→ `"Lit"`), falling back to `"Off"`. Previously only checked the physical LED
  state, never returning `"Blinking"`. Maps to `redfish-core/lib/led.hpp`.

### Tests

- 157 unit tests passing (up from 149 at v0.4.0). Zero failures.
- New tests for OData document, all Fabric endpoints, NetworkAdapter instance,
  StorageController, OperatingConfigs, DBusEventLog service + entries.

---

## [0.4.0] - 2026-07-15

### Added

- **10 new upstream Redfish endpoints (Round 3 upstream sync)**

  - `GET /Systems/{id}/Storage/{storage_id}` — Storage controller instance with
    drive enumeration from `Inventory.Item.Drive` DBus objects
    (`redfish-core/lib/storage.hpp`)

  - `GET /Chassis/{id}/PowerSubsystem/PowerSupplies/{psu_id}` — Individual power
    supply instance; reads state, input/output wattage, and firmware version from
    DBus (`redfish-core/lib/power_supply.hpp`)

  - `GET /Chassis/{id}/ThermalSubsystem/ThermalMetrics` — Temperature sensor
    readings from DBus sensor tree (`redfish-core/lib/thermal_metrics.hpp`)

  - `GET /Chassis/{id}/PCIeSlots` — PCIe slot inventory from
    `Inventory.Item.PCIeSlot` DBus objects (`redfish-core/lib/pcie_slots.hpp`)

  - `GET /Systems/hypervisor` — IBM POWER hypervisor partition stub; returns 404
    when no hypervisor DBus object present
    (`redfish-core/lib/hypervisor_system.hpp`)

  - `GET /Managers/{id}/LogServices/Journal[/Entries]` — Systemd journal log
    service; entries read from `journalctl -o short-precise -n 200` with graceful
    degradation to empty list when `journalctl` is unavailable
    (`redfish-core/lib/manager_logservices_journal.hpp`). Manager `LogServices`
    collection count updated 2 → 3.

  - `GET /AggregationService` — Aggregation service stub with
    `ServiceEnabled: false` (`redfish-core/lib/aggregation_service.hpp`)

- **IPMI ProtocolEnabled from DBus** (`managers.rs`) — `NetworkProtocol.IPMI.ProtocolEnabled`
  now reads the `Running` property from
  `xyz.openbmc_project.Control.Service.Attributes` on the
  `/control/service/phosphor_2dipmi_2dnet` object; falls back to `true` when
  the property is unavailable. Matches upstream commit `9352bdc8`.

- **Account ConfigureSelf password PATCH** (`src/api/redfish/accounts.rs`) —
  `PATCH /AccountService/Accounts/{id}` now permits callers without
  `ConfigureUsers` to update only their own `Password`. Any other field, or any
  patch to a different account, still returns `403 Forbidden`.

- **Processor EnvironmentMetrics PATCH** (`src/api/redfish/systems.rs`) —
  `PATCH /Systems/{id}/Processors/{id}/EnvironmentMetrics` now accepts
  `PowerLimitWatts.SetPoint` and writes it to
  `xyz.openbmc_project.Control.Power.Cap.PowerCap` when the matching processor
  power-cap interface is present.

- **Manager EthernetInterface PATCH privilege** (`src/api/redfish/managers.rs`) —
  `PATCH /Managers/{id}/EthernetInterfaces/{id}` now requires
  `ConfigureComponents` instead of the generic manager PATCH privilege, matching
  upstream privilege registration.

- **Manager NetworkProtocol SSH state** (`src/api/redfish/managers.rs`) —
  `GET /Managers/{id}/NetworkProtocol` now reads `SSH.ProtocolEnabled` from the
  dropbear `Running` property when available and continues to fall back to
  enabled-by-default behavior when the backing DBus object is absent.

- **Manager DBusEventLog entry route** (`src/api/redfish/managers.rs`) —
  `GET /Managers/{id}/LogServices/DBusEventLog/Entries/{entry_id}` now returns a
  concrete entry resource when the matching DBus log object exists, while the
  collection path continues to return an empty list cleanly when no DBus entries
  are available.

- **`PasswordExpirationDays` PATCH** (`accounts.rs`) — `PATCH /AccountService/Accounts/{id}`
  now accepts `PasswordExpirationDays` (uint64). Writes `UserPasswordExpiry` via
  `set_property` on `xyz.openbmc_project.User.Attributes`.

### Fixed

- **axum 0.7 catch-all routing** (`src/api/dbus_rest.rs`) — DBus REST routes
  `/xyz/{*path}` and `/org/{*path}` used axum 0.8 wildcard syntax, causing
  a startup panic: `"Invalid route: catch-all parameters are only allowed at
  the end of a route"`. Corrected to `*path` (axum 0.7 syntax).

- **ServiceRoot unauthenticated access** (`src/api/redfish/mod.rs`,
  `src/protocol/http.rs`) — `GET /redfish/v1` was behind the mandatory auth
  middleware, requiring credentials and violating Redfish spec §7.3.1.
  Moved to the open (optional-auth) router. Added `/redfish/v1/` trailing-slash
  alias for compatibility with the DMTF Redfish Service Validator.

### Changed

- **`.gitignore`** — Added scratch/commit helper file patterns (`_commitmsg.txt`,
  `_msg_fix.py`, `_filter_callback.py`, `_replace_msg.txt`) to prevent accidental
  commits of ephemeral development tooling.

- **`DEVELOPMENT_STATUS.md`** — Updated "Not Yet Implemented" section to
  reflect completed features: DBus REST API, KVM WebSocket, Virtual Media,
  mTLS, Registries/JsonSchemas are all implemented. Only LDAP integration
  and complete Virtual Media data-path remain open. Added v0.4.0 performance
  table rows (60+ routes, 149 unit tests).

- **DMTF Redfish Service Validator integration** (`scripts/_run_validator.sh`)
  — New script that boots rainier-bmc QEMU, injects bmcweb-ng, and runs the
  DMTF `RedfishServiceValidator.py` against the live service.

- **Expanded e2e test suite** (`scripts/_e2e_test.py`) — Grew to 69 tests
  covering all major Redfish endpoints, with path auto-detection from `__file__`
  and `SKIP_TEARDOWN=1` mode for external tooling integration.

- **QEMU_SETUP.md** — Added full p10bmc / IBM Rainier section.

- **`.cargo/config.toml`** — Clarified cross-compilation targets.

### Tests

- 149 unit tests passing (up from 134 at v0.3.0). Zero failures.
- New tests for Journal LogService, AggregationService, PSU instance,
  ThermalMetrics, PCIeSlots, Storage instance, Hypervisor endpoint.

---

## [0.3.0] - 2026-07-11

### Added

- **CODING_STANDARDS.md expanded** — Four new sections: Async Patterns
  (`tokio::spawn` vs inline await, `Arc` sharing, blocking-I/O prohibition),
  Redfish Response Conventions (`@odata` fields, collection shape, error
  format, dynamic `@odata.id`), Logging (level guidance table with examples),
  and Security (session tokens, per-handler `check_privilege()` pattern, input
  validation rules, TLS cert verification requirement).

- **Per-route RBAC enforcement** (`auth/privilege.rs`, all handler files) —
  `check_privilege()` wired into every PATCH, POST (action/create), and DELETE
  handler across the Redfish API.  `Extension<UserSession>` added to all
  mutating handler signatures.  `delete_session` enforces own-session vs
  ConfigureUsers privilege correctly.

- **KVM WebSocket proxy** (`api/websocket/mod.rs`) — `/kvm/0` now proxies
  bidirectionally to `obmc-ikvm` on `127.0.0.1:5900` (TCP) matching upstream
  `features/kvm/kvm_websocket.hpp`.  Replaces the 1011-close stub.

- **Virtual Media WebSocket endpoints** (`api/websocket/mod.rs`) — `/vm/0/0`
  and `/nbd/0` added as UNIX-socket proxies to `/run/media-proxy/slot_0` and
  `/run/media-proxy/nbd_0` respectively.  Buffer size 128 KiB + 16 bytes
  (NBD max message per protocol spec).

- **DBus REST API** (`api/dbus_rest.rs`) — New module implementing the upstream
  `openbmc_dbus_rest.hpp` feature:
  - `GET /bus/` — list buses
  - `GET /bus/system/` — list DBus service names via `ListNames`
  - `GET /list/` — enumerate all objects via `GetManagedObjects`
  - `GET /xyz/<path>`, `GET /org/<path>` — get all properties of a DBus object
  - `PUT /xyz/<path>`, `PUT /org/<path>` — set a property via `set_property`
  Routes mounted at root behind mandatory auth middleware.

- **Static WebUI file serving** (`protocol/http.rs`) — `ServeDir` mounted at
  `/ui` serving from `/usr/share/www` (OpenBMC) or `./www` (dev fallback).
  `ServeFile` fallback to `index.html` for SPA client-side routing.

- **Mutual TLS (mTLS) authentication** (`config/mod.rs`, `protocol/http.rs`,
  `auth/middleware.rs`) — `mtls_enabled` and `mtls_ca_cert` fields added to
  `ServerConfig` (serde-defaulted, backward compatible).  `build_mtls_config()`
  uses `WebPkiClientVerifier` to require client certs signed by the configured
  CA.  Peer certificate Subject CN extracted via DER byte walk and injected as
  `X-Client-Cert-Subject` header; auth middleware creates a session from the CN.

- **Registries and JsonSchemas full content** (`api/redfish/service_root.rs`,
  `api/redfish/mod.rs`) — Replaces empty stubs with:
  - 5 registries: Base v1.17.0, TaskEvent v1.0.3, ResourceEvent v1.3.0,
    HeartbeatEvent v1.0.1, OpenBMC v1.0.0
  - 26 JsonSchemas covering all resource types implemented in bmcweb-ng
  - New `GET /Registries/{id}` and `GET /JsonSchemas/{id}` endpoints returning
    `MessageRegistryFile` and `JsonSchemaFile` resources with DMTF URIs.

### Tests

- 134 unit tests passing (up from 122 at v0.2.1).
- 63/63 QEMU integration tests passing.
- Zero clippy warnings.

---

## [0.2.1] - 2026-07-11

### Added

- **NetworkAdapters DBus enumeration** (`chassis.rs`) —
  `GET /Chassis/{chassis_id}/NetworkAdapters` now queries
  `xyz.openbmc_project.Inventory.Item.NetworkAdapter` via `GetManagedObjects`
  on `xyz.openbmc_project.Inventory.Manager`, filtering to objects under the
  chassis inventory path.  Falls back to an empty collection when DBus is
  unavailable.  `@odata.id` now uses the dynamic `chassis_id` parameter.

- **EventService PATCH persistence** (`services/event.rs`, `event_service.rs`) —
  `DeliveryRetryAttempts` and `DeliveryRetryIntervalSeconds` are now stored in
  `EventServiceSettings` behind an `RwLock` on `EventService`.  `PATCH
  /redfish/v1/EventService` calls `update_settings()` so subsequent GET calls
  return the updated values instead of hard-coded defaults.

- **Server-Sent Events endpoint** (`event_service.rs`, `redfish/mod.rs`) —
  `GET /redfish/v1/EventService/SSE` implemented per upstream bmcweb
  `eventservice_sse.hpp`.  Returns an axum `Sse` stream that sends a single
  heartbeat event on connect.  `ServerSentEventUri` field added to the
  EventService GET response.

- **ZBusClient::call_method fully implemented** (`dbus/mod.rs`) —
  `call_method` now dispatches on the JSON argument shape (None / String / scalar /
  array) and converts each to the correct `zvariant` type.  Added
  `call_method_hetero_array` free-function helper for heterogeneous 3-element arrays
  (`(s as b)` signatures used by `xyz.openbmc_project.User.Manager.CreateUser`).
  Previously the method always returned an error for non-trivial argument lists.

- **SessionTimeout persistence** (`auth/session.rs`, `api/redfish/sessions.rs`) —
  `SessionStore.timeout_seconds` is now an `Arc<AtomicI64>` shared between all
  code paths.  `timeout_seconds()` and `set_timeout_seconds()` public accessors
  added.  `GET /SessionService` reads the live value; `PATCH /SessionService` with
  `{"SessionTimeout": N}` persists it for the lifetime of the process.

- **Structured JSON /health endpoint** (`protocol/http.rs`) —
  `GET /health` now returns a JSON document with per-component health:
  `dbus`, `sessions`, `metrics`.  Each component reports `"ok"` or `"degraded"`
  with a detail string.  The top-level `status` is `"degraded"` if any required
  component (dbus, sessions) is unavailable.  Includes `version` field from
  `CARGO_PKG_VERSION`.

### Changed

- `config/mod.rs`: removed unused `FeaturesConfig` struct and `methods` field
  from `AuthConfig`; removed `format` field from `LoggingConfig`.  These fields
  were present in `config.toml` and the struct but were never read by any handler.
- `config.toml`: removed `[features]` section and `methods` key from `[auth]`;
  removed `format` from `[logging]`; added explanatory comments.
- Stale `// TODO:` comments removed from `api/mod.rs`, `auth/mod.rs`,
  `services/mod.rs`, `observability/mod.rs`, and `api/websocket/mod.rs`.
  Replaced with accurate notes describing where each feature lives or is planned.

### Tests

- 121 unit tests passing; `test_health_handler` updated to exercise the new
  JSON response from the structured `/health` endpoint.

---

## [0.2.0] - 2026-07-11

### Added

- **Chassis live inventory from DBus** (`chassis.rs`) —
  `GET /Chassis/{id}` reads `Name`, `Model`, `SerialNumber`, `PartNumber` from
  `xyz.openbmc_project.Inventory.Decorator.Asset` and `Inventory.Item.PrettyName`.
  `IndicatorLED` reads from `xyz.openbmc_project.Led.Physical.State`.
  All `@odata.id` sub-resource links now use the dynamic `chassis_id`.

- **PATCH /Chassis/{id} IndicatorLED** (`chassis.rs`) —
  `PATCH /Chassis/{id}` writes `Asserted` (bool) on
  `xyz.openbmc_project.Led.Group` at `/xyz/openbmc_project/led/groups/front_id`.

- **PowerControl total wattage** (`chassis.rs`) —
  `PowerConsumedWatts` in the `PowerControl` array reads live value from
  `/xyz/openbmc_project/sensors/power/total_power` via `get_property`.

- **System AssetTag/SerialNumber/Model from DBus** (`systems.rs`) —
  `GET /Systems/system` reads `AssetTag` from `Inventory.Decorator.AssetTag`,
  and `SerialNumber`, `PartNumber`, `Model` from `Inventory.Decorator.Asset`.
  `PATCH /Systems/system` applies `AssetTag` changes via `set_property`.

- **FirmwareInventory enriched from DBus** (`update_service.rs`) —
  `GET /UpdateService/FirmwareInventory` enumerates live software images from
  `xyz.openbmc_project.Software.BMC.Updater` using `GetManagedObjects`.
  Results are deduplicated with the in-memory firmware list from upload operations.

- **Storage collection from DBus** (`systems.rs`) —
  `GET /Systems/system/Storage` now enumerates storage controller objects via
  `GetManagedObjects` filtering on `Inventory.Item.StorageController`.
  When no explicit controller objects exist, synthesises one "Storage/1" entry
  if any `Inventory.Item.Drive` objects are present.

- **PATCH EthernetInterface** (`managers.rs`) —
  `PATCH /Managers/bmc/EthernetInterfaces/{nic_id}` handles `DHCPv4.DHCPEnabled`,
  `MACAddress`, and `IPv4StaticAddresses`. Static IPs call
  `xyz.openbmc_project.Network.IP.Create / IP` via `call_method`.
  Returns the updated NIC resource after applying changes.

- **Dynamic NIC validation** (`managers.rs`) —
  `GET /Managers/bmc/EthernetInterfaces/{nic_id}` now validates the NIC id
  against the live DBus NIC list (via `GetManagedObjects`) rather than
  hard-coding `eth0`.

- **Boot override settings** (`systems.rs`) — `GET /Systems/system` now reads
  `BootSource` from `xyz.openbmc_project.Control.Boot.Source` at
  `/xyz/openbmc_project/control/host0/boot` and the one-time-boot path.
  `BootSourceOverrideEnabled`, `BootSourceOverrideTarget`, and `BootSourceOverrideMode`
  are populated from live DBus values instead of static defaults.

- **PATCH /Systems/system** (`systems.rs`) — New handler allows setting
  `Boot.BootSourceOverrideTarget` and `Boot.BootSourceOverrideEnabled` via DBus
  `set_property` on the persistent boot path and one-time-boot path.

- **EventLog Entries collection** (`systems.rs`) —
  `GET /Systems/system/LogServices/EventLog/Entries` reads all log entries from
  `xyz.openbmc_project.Logging` via `GetManagedObjects`, returning them newest-first
  with `Severity`, `Created`, and `Message` fields.

- **EventLog Entry instance** (`systems.rs`) —
  `GET /Systems/system/LogServices/EventLog/Entries/{entry_id}` reads a single log
  entry from `/xyz/openbmc_project/logging/entry/{N}` via `get_all_properties`.

- **ClearLog action** (`systems.rs`) —
  `POST /Systems/system/LogServices/EventLog/Actions/LogService.ClearLog` calls
  `xyz.openbmc_project.Collection.DeleteAll / DeleteAll` to flush all log entries.

- **PATCH NetworkProtocol fully wired** (`managers.rs`) —
  `PATCH /Managers/bmc/NetworkProtocol` now applies `HostName` and `NTP.NTPServers`
  changes via `set_property` on `xyz.openbmc_project.Network.SystemConfiguration`.
  Returns the updated resource after applying changes.

- **AccountService DBus wiring** (`accounts.rs`) —
  - `GET /AccountService/Accounts` now calls `xyz.openbmc_project.User.Manager / ListUsers`
    to enumerate all BMC users dynamically. Falls back to static `root` entry when DBus is
    unavailable.
  - `POST /AccountService/Accounts` calls `CreateUser(username, [priv-group, ssh], enabled)`
    to create a real system user. Returns HTTP 500 if DBus call fails.
  - `GET /AccountService/Accounts/{id}` calls `GetUserInfo(username)` to read `UserPrivilege`,
    `UserEnabled`, and `UserLockedForFailedAttempt`. Falls back to static `root` data for
    unknown users when DBus is unavailable.
  - `PATCH /AccountService/Accounts/{id}` applies `RoleId` and `Enabled` changes via
    `set_property` on `xyz.openbmc_project.User.Attributes`.
  - `DELETE /AccountService/Accounts/{id}` calls `DeleteUser(username)`. Deleting `root`
    is still forbidden (returns HTTP 403).
  - Added `openbmc_priv_to_role()` helper mapping OpenBMC priv groups to Redfish roles.

- **Chassis Power DBus wiring** (`chassis.rs`) — `GET /Chassis/chassis/Power` now queries
  `GetManagedObjects` for objects under `/xyz/openbmc_project/sensors/voltage/` (voltage
  sensors, `Sensor.Value` interface) and inventory objects with
  `Inventory.Item.PowerSupply`. `Voltages` and `PowerSupplies` arrays are populated from
  live DBus data; fall back to empty arrays when DBus is unavailable.

- **Chassis Thermal DBus wiring** (`chassis.rs`) — `GET /Chassis/chassis/Thermal` now
  queries `GetManagedObjects` on the sensor service for temperature sensors
  (`/sensors/temperature/`) and fan sensors (`/sensors/fan*`). `Temperatures` and `Fans`
  arrays include `ReadingCelsius`/`Reading` values, threshold properties, and `ReadingUnits`.
  Falls back to empty arrays when DBus is unavailable.

- **Chassis Sensors DBus enumeration** (`chassis.rs`) — `GET /Chassis/chassis/Sensors` now
  enumerates all objects with `Sensor.Value` interface under `/xyz/openbmc_project/sensors/`
  and returns a `SensorCollection` with `Members@odata.count` reflecting the live count.
  Sensor IDs are derived from the last two path segments (e.g. `temperature_ambient`).

- **BMC reset DBus wiring** (`managers.rs`) — `POST /Managers/bmc/Actions/Manager.Reset`
  now writes `RequestedBMCTransition` on `xyz.openbmc_project.State.BMC` at
  `/xyz/openbmc_project/state/bmc0`. `GracefulRestart` maps to `Transition.Reboot`;
  `ForceRestart` maps to `Transition.HardReboot`. Returns 204 even if DBus is unavailable
  (logs a warning).

- **System reset DBus wiring** (`systems.rs`) — `POST /Systems/system/Actions/ComputerSystem.Reset`
  now writes `RequestedHostTransition` or `RequestedPowerTransition` to the appropriate
  DBus state object. Full ResetType to DBus transition mapping:
  `On`/`ForceOn` to `Host.Transition.On`, `ForceOff` to `Chassis.Transition.Off`,
  `GracefulShutdown` to `Host.Transition.Off`, `GracefulRestart` to `Host.Transition.Reboot`,
  `ForceRestart` to `Host.Transition.ForceWarmReboot`, `Nmi` to `Host.Transition.DiagnosticMode`.

- **BMC NIC enumeration** (`managers.rs`) — `GET /Managers/bmc/EthernetInterfaces`
  now calls `GetManagedObjects` on `xyz.openbmc_project.Network` to enumerate all
  interfaces implementing `xyz.openbmc_project.Network.EthernetInterface`. Falls back to
  a single `eth0` entry when DBus is unavailable or returns an empty set.

- **DBus PowerState wiring** (`systems.rs`) — `GET /redfish/v1/Systems/system`
  now queries `xyz.openbmc_project.State.Host / CurrentHostState` at
  `/xyz/openbmc_project/state/host0`. Maps `HostState.Running`, `Quiesced`, and
  `DiagnosticMode` to `"On"`; `HostState.Off` to `"Off"`; unknown states to
  `"Unknown"`. Gracefully falls back to `"Unknown"` when DBus is unavailable.

- **DBus FirmwareVersion wiring** (`managers.rs`) — `GET /redfish/v1/Managers/bmc`
  now queries `xyz.openbmc_project.Software.Version / Version` on the active BMC
  image object at `/xyz/openbmc_project/software/active`. Falls back to `"Unknown"`.

- **DBus hostname + NTP wiring** (`managers.rs`) — `GET /redfish/v1/Managers/bmc/NetworkProtocol`
  now queries `xyz.openbmc_project.Network.SystemConfiguration / HostName` and
  `NTPServers` from `/xyz/openbmc_project/network/config`. `HostName` and `FQDN`
  fields reflect the live hostname; `NTP.NTPServers` contains the current NTP list.

- **DBus MAC/IP wiring** (`managers.rs`) — `GET /redfish/v1/Managers/bmc/EthernetInterfaces/eth0`
  now queries `xyz.openbmc_project.Network.EthernetInterface / MACAddress`,
  `IPv4Addresses`, and `IPv6Addresses` from `/xyz/openbmc_project/network/eth0`.
  Falls back to `00:00:00:00:00:00` / empty lists when DBus is unavailable.

- **Role-based session support** (`auth/session.rs`, `auth/privilege.rs`) — `UserSession`
  now carries a `role: String` field (default `"ReadOnly"`), set at session-creation
  time via `set_role()`. `session_role()` in `privilege.rs` reads the stored role
  instead of hard-coding `"ReadOnly"`. `SessionStore::set_session_role()` allows
  updating the persisted role in-store after creation. New tests confirm Administrator
  can configure, ReadOnly cannot, and that the stored role is correctly used.

- **DBus role lookup on login** (`sessions.rs`) — `POST /redfish/v1/SessionService/Sessions`
  now calls `xyz.openbmc_project.User.Manager / GetUserInfo` after PAM authentication
  to look up the user's `priv-admin`, `priv-operator`, `priv-user`, or `priv-noaccess`
  group, mapping it to a Redfish role (`Administrator`, `Operator`, `ReadOnly`,
  `NoAccess`). Falls back to `"ReadOnly"` if DBus is unavailable or call fails.

- **LogServices/EventLog instance endpoint** (`systems.rs`, `mod.rs`) —
  `GET /redfish/v1/Systems/system/LogServices/EventLog` now returns a fully-formed
  `LogService.v1_4_0.LogService` resource with `Entries` link and `ClearLog` action
  target. Route registered in `mod.rs`.

- **`ZBusClient::set_property()` implementation** (`dbus/mod.rs`) — Previously
  returned an immediate error. Now builds a `PropertiesProxy`, converts the
  `serde_json::Value` to a `zbus::zvariant::Value` via `json_to_zvariant()`, and
  calls `org.freedesktop.DBus.Properties.Set`. Supports string, bool, integer,
  double, and string-array JSON types. Object and null values return a descriptive
  error. Six new unit tests cover all conversion paths.

- **DBus chassis enumeration** (`chassis.rs`) — `GET /redfish/v1/Chassis` now queries
  `xyz.openbmc_project.Inventory.Manager / GetManagedObjects` and returns all objects
  that implement `xyz.openbmc_project.Inventory.Item.Chassis`, ordered lexicographically.
  Falls back to a single `"chassis"` member when DBus is unavailable or the inventory
  is empty. `GET /redfish/v1/Chassis/{id}` validates the chassis ID against the same
  inventory, also accepting the default `"chassis"` ID as a fallback.

- **Processor and Memory instance endpoints** (`systems.rs`, `mod.rs`) —
  `GET /redfish/v1/Systems/system/Processors/{id}` returns a `Processor.v1_16_0.Processor`
  resource with `Model`, `TotalCores`, and `TotalThreads` pulled from
  `xyz.openbmc_project.Inventory.Item.Cpu` in DBus. Returns 404 if the ID is not found
  in the inventory.
  `GET /redfish/v1/Systems/system/Memory/{id}` returns a `Memory.v1_18_0.Memory`
  resource with `CapacityMiB`, `OperatingSpeedMhz`, and `MemoryType` pulled from
  `xyz.openbmc_project.Inventory.Item.Dimm`. Both routes registered in `mod.rs`.
  The Processors and Memory collections are updated to enumerate live DBus inventory.

- **SessionService API** — Full Redfish SessionService resource family
  (GET/PATCH /redfish/v1/SessionService, GET/POST /redfish/v1/SessionService/Sessions,
  GET/DELETE /redfish/v1/SessionService/Sessions/{id}). Session creation via PAM,
  returns `X-Auth-Token` header. Includes /Members alias per DSP0266.

- **AccountService API** — Complete AccountService resource family including
  Accounts collection (GET/POST), individual account management (GET/PATCH/DELETE),
  Roles collection, and individual Role resources. All four Redfish built-in roles
  (Administrator, Operator, ReadOnly, NoAccess) with correct privilege mappings.

- **EventService API** — EventService resource family wired to the EventService
  business layer: GET/PATCH EventService, SubmitTestEvent action, Subscriptions
  collection (GET/POST), individual subscription management (GET/DELETE).

- **TaskService API** — TaskService resource family: GET TaskService, Tasks
  collection (GET), individual task (GET/DELETE). Task state/progress/messages
  exposed per Redfish TaskService schema v1.2.0.

- **UpdateService API** — UpdateService with FirmwareInventory collection (GET),
  individual firmware items (GET), and SimpleUpdate action (POST). SimpleUpdate
  creates a task and returns 202 Accepted with Location header pointing at the task.

- **Privilege/RBAC system** — Redfish PrivilegeRegistry-conformant RBAC per
  DSP0272. Five standard privileges, four built-in roles with correct assignments,
  `check_privilege()` helper for route handlers, route-level privilege constants.

- **ZBusClient** — Production DBus client using `org.freedesktop.DBus.Properties`
  and `ObjectManager` proxies. `MockDbusClient` for unit testing with pre-populated
  property maps. `zvariant_to_json` type conversion helper.

- **TLS/HTTPS support** — `rustls`-backed HTTPS server with PEM certificate loading,
  automatic fallback to self-signed generation, HTTP/2 via ALPN,
  TLS-based accept loop with per-connection `tokio::spawn`.

- **Chassis sub-resources** — Power (PSUs, voltage sensors), Thermal (temperatures,
  fans), Sensors collection, NetworkAdapters collection. All document OpenBMC DBus
  interface paths for future integration.

- **Systems sub-resources** — Processors collection, Memory collection, Storage
  collection, EthernetInterfaces collection, LogServices collection. Each endpoint
  documents the relevant OpenBMC DBus inventory interface.

- **Managers sub-resources** — NetworkProtocol (GET/PATCH with NTP/SSH/HTTPS config),
  EthernetInterfaces collection and individual NIC, LogServices collection. NIC
  endpoint includes full DHCPv4/v6 and IPv4/v6 fields.

- **WebSocket foundation** — Serial console handler (`/console0`) that proxies
  bidirectional byte streams between WebSocket and the obmc-console UNIX socket at
  `/run/obmc-console/default`. KVM handler stub at `/kvm/0`.

- **Authentication middleware integration** — `optional_auth_middleware` applied
  globally to Redfish and WebSocket routes. Session creation endpoints intentionally
  receive unauthenticated requests. WebSocket auth checked post-upgrade.

- **Persistent data store** — `PersistentStore` reads/writes `/var/lib/bmcweb/config.json`
  with atomic rename-based writes. System UUID is generated once and persisted across
  restarts. Format is versioned JSON matching upstream bmcweb's persistent_data convention.

- **`extract_client_ip()`** — Public helper in auth middleware for consistent IP
  extraction across session creation and middleware layers.

### Changed

- `accounts.rs`: removed hard-coded `root`-only restriction from `get_account()`; now
  any account present in DBus can be retrieved.
- `chassis.rs`: Power, Thermal, and Sensors endpoints now use dynamic `@odata.id` based on
  `chassis_id` parameter instead of hard-coded `"/redfish/v1/Chassis/chassis/..."`.
- `managers.rs`: EthernetInterfaces collection count is now dynamic (reflects live NIC count).
- `systems.rs` Processors and Memory collections now dynamically enumerate DBus
  inventory objects rather than returning hard-coded empty collections.
- `privilege.rs` `session_role()` now reads `session.role` rather than returning
  a hard-coded `"ReadOnly"` for all sessions.
- All DBus-querying handlers use the `ZBusClient::from_connection()` constructor
  with graceful fallback when `AppState.dbus_connection` is `None`.
- Updated all repository URLs from internal to public GitHub (`https://github.com/gtmills/bmcweb-ng`)
- Improved `managers.rs`: added NetworkProtocol, EthernetInterfaces, LogServices
  sub-resource links, expanded SerialConsole/CommandShell/GraphicalConsole fields
- Improved `systems.rs`: added HealthRollup, AllowableValues for ResetType,
  expanded sub-resource link list
- Improved `chassis.rs`: added HealthRollup, Assembly link
- `protocol/mod.rs`: removed stale TODO comments, re-exported `HttpServer`
- `auth/mod.rs`: exported `extract_client_ip`, `check_privilege`, `privileges_for_role`

### Fixed

- Service root test assertion now matches actual `RedfishVersion` value
- `get_client_ip()` made private; public `extract_client_ip()` wrapper added

### Security

- RBAC privilege checking infrastructure in place
- Session tokens use UUID v4 (119-bit entropy, above OWASP minimum of 64 bits)
- Redfish role stored per-session and read at privilege check time

### Tests

- 115 unit tests passing; all DBus-integration paths covered with no-DBus fallback
  tests; new coverage for privilege checks with specific roles, `json_to_zvariant`
  conversion, EventLog endpoint, PowerState mapping, account operations.

---

## [0.1.0] - 2026-04-28

### Added

- Initial project setup
- Basic Rust project structure
- README with project overview and roadmap
- Apache 2.0 license
- Git repository initialization

[Unreleased]: https://github.com/gtmills/bmcweb-ng/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/gtmills/bmcweb-ng/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/gtmills/bmcweb-ng/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/gtmills/bmcweb-ng/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/gtmills/bmcweb-ng/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/gtmills/bmcweb-ng/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/gtmills/bmcweb-ng/releases/tag/v0.1.0
