#!/bin/bash
# Test bmcweb-ng in QEMU via SSH tunnel
set -e

SSH="sshpass -p 0penBmc ssh -o StrictHostKeyChecking=no -p 2222 root@127.0.0.1"

PASS=0
FAIL=0

echo "=== bmcweb-ng QEMU Smoke Test ==="

# Confirm bmcweb-ng is running on port 8080
PID=$($SSH 'ps | grep bmcwebd-ng | grep -v grep | head -1 | awk "{print \$1}"' 2>/dev/null || echo "")
if [ -z "$PID" ]; then
    echo "Starting bmcweb-ng..."
    $SSH 'RUST_LOG=warn /tmp/bmcwebd-ng --config /tmp/ng-test.toml > /tmp/ng-test.log 2>&1 &'
    sleep 3
else
    echo "bmcweb-ng already running (pid $PID)"
fi

# Set up SSH tunnel: localhost:18080 → QEMU:8080
echo "Opening SSH tunnel on port 18080..."
sshpass -p 0penBmc ssh -o StrictHostKeyChecking=no -p 2222 -L 18080:localhost:8080 -N root@127.0.0.1 &
TUNNEL_PID=$!
sleep 2

BASE="http://127.0.0.1:18080"

check() {
    local name="$1"
    local url="$2"
    local pattern="$3"
    local auth="${4:-}"
    
    local curl_args="-s --max-time 10"
    if [ -n "$auth" ]; then
        curl_args="$curl_args -u $auth"
    fi
    
    RESP=$(curl $curl_args "$url" 2>/dev/null || echo "CURL_FAILED")
    if echo "$RESP" | grep -q "$pattern"; then
        echo "PASS: $name"
        PASS=$((PASS+1))
    else
        echo "FAIL: $name"
        echo "  Response: ${RESP:0:200}"
        FAIL=$((FAIL+1))
    fi
}

check_code() {
    local name="$1"
    local url="$2"
    local expected_code="$3"
    local auth="${4:-}"
    
    local curl_args="-s -o /dev/null -w %{http_code} --max-time 10"
    if [ -n "$auth" ]; then
        curl_args="$curl_args -u $auth"
    fi
    
    CODE=$(curl $curl_args "$url" 2>/dev/null || echo "000")
    if [ "$CODE" = "$expected_code" ]; then
        echo "PASS: $name (HTTP $CODE)"
        PASS=$((PASS+1))
    else
        echo "FAIL: $name (expected HTTP $expected_code, got $CODE)"
        FAIL=$((FAIL+1))
    fi
}

AUTH="root:0penBmc"

echo ""
echo "--- Core Redfish endpoints ---"
check "ServiceRoot unauthenticated" "$BASE/redfish/v1" "ServiceRoot"
check_code "Systems unauthenticated returns 401" "$BASE/redfish/v1/Systems" "401"
check "Systems collection" "$BASE/redfish/v1/Systems" "ComputerSystemCollection" "$AUTH"
check "System instance" "$BASE/redfish/v1/Systems/system" '"Id":"system"' "$AUTH"
check "System PowerState" "$BASE/redfish/v1/Systems/system" "PowerState" "$AUTH"
check "Chassis collection" "$BASE/redfish/v1/Chassis" "ChassisCollection" "$AUTH"
check "Chassis instance" "$BASE/redfish/v1/Chassis/chassis" '"Id":"chassis"' "$AUTH"
check "Managers collection" "$BASE/redfish/v1/Managers" "ManagerCollection" "$AUTH"
check "Manager bmc" "$BASE/redfish/v1/Managers/bmc" '"ManagerType":"BMC"' "$AUTH"
check "AccountService" "$BASE/redfish/v1/AccountService" "AccountService" "$AUTH"
check "EventService" "$BASE/redfish/v1/EventService" '"ServiceEnabled":true' "$AUTH"
check "TaskService" "$BASE/redfish/v1/TaskService" "TaskService" "$AUTH"
check "UpdateService" "$BASE/redfish/v1/UpdateService" "UpdateService" "$AUTH"
check "FirmwareInventory" "$BASE/redfish/v1/UpdateService/FirmwareInventory" "SoftwareInventoryCollection" "$AUTH"

echo ""
echo "--- Phase 5 new endpoints ---"
check "VirtualMedia collection" "$BASE/redfish/v1/Managers/bmc/VirtualMedia" "VirtualMediaCollection" "$AUTH"
check "Systems EthernetInterfaces" "$BASE/redfish/v1/Systems/system/EthernetInterfaces" "EthernetInterfaceCollection" "$AUTH"
check "Systems NetworkInterfaces" "$BASE/redfish/v1/Systems/system/NetworkInterfaces" "NetworkInterfaceCollection" "$AUTH"
check "Chassis Assembly" "$BASE/redfish/v1/Chassis/chassis/Assembly" "Assembly" "$AUTH"
check "Chassis Power" "$BASE/redfish/v1/Chassis/chassis/Power" "PowerControl" "$AUTH"
check "Systems Storage" "$BASE/redfish/v1/Systems/system/Storage" "StorageCollection" "$AUTH"
check "LogServices" "$BASE/redfish/v1/Systems/system/LogServices" "LogServiceCollection" "$AUTH"
check_code "Systems hypervisor 404" "$BASE/redfish/v1/Systems/hypervisor" "404" "$AUTH"

echo ""
echo "--- DBus-wired endpoints ---"
check "Managers FW version present" "$BASE/redfish/v1/Managers/bmc" "FirmwareVersion" "$AUTH"
check "System Boot fields" "$BASE/redfish/v1/Systems/system" "BootSourceOverrideTarget" "$AUTH"
check "EthernetInterfaces count>0" "$BASE/redfish/v1/Managers/bmc/EthernetInterfaces" "eth0" "$AUTH"
check "NetworkProtocol" "$BASE/redfish/v1/Managers/bmc/NetworkProtocol" "ManagerNetworkProtocol" "$AUTH"

echo ""
echo "==================================="
echo "Results: PASS=${PASS}  FAIL=${FAIL}"
echo "==================================="

# Clean up tunnel
kill $TUNNEL_PID 2>/dev/null || true

if [ "$FAIL" -eq 0 ]; then
    echo ""
    echo "ALL ${PASS} TESTS PASSED"
    exit 0
else
    echo ""
    echo "${FAIL} TESTS FAILED (${PASS} passed)"
    exit 1
fi
