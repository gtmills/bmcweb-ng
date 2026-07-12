#!/bin/bash
set -e
BMCWEB="https://127.0.0.1:2443"
NG="http://127.0.0.1:8080"
AUTH="-u admin:0penBmc2!"

echo "=== Waiting for upstream bmcweb (up to 300s) ==="
for i in $(seq 1 60); do
  CODE=$(curl -sk --max-time 3 -o /dev/null -w '%{http_code}' "$BMCWEB/redfish/v1" 2>/dev/null || echo "000")
  echo "  [${i}] HTTP $CODE"
  if [ "$CODE" = "200" ] || [ "$CODE" = "401" ] || [ "$CODE" = "403" ]; then break; fi
  sleep 5
done

# Patch admin password if needed
CODE=$(curl -sk $AUTH --max-time 5 -o /dev/null -w '%{http_code}' "$BMCWEB/redfish/v1/Systems" 2>/dev/null || echo "000")
if [ "$CODE" != "200" ]; then
  echo "=== Fixing admin credentials ==="
  for TRY_PW in "admin" "0penBmc"; do
    PATCH_CODE=$(curl -sk --max-time 5 -o /dev/null -w '%{http_code}' \
      -X PATCH -H 'Content-Type: application/json' \
      -u "admin:${TRY_PW}" \
      -d '{"Password":"0penBmc2!"}' \
      "$BMCWEB/redfish/v1/AccountService/Accounts/admin" 2>/dev/null || echo "000")
    echo "  PATCH with $TRY_PW → HTTP $PATCH_CODE"
    if [ "$PATCH_CODE" = "200" ] || [ "$PATCH_CODE" = "204" ]; then break; fi
  done
  sleep 2
fi

echo ""
echo "============================================================"
echo "  /redfish/v1 — SERVICE ROOT"
echo "============================================================"
echo ""
echo "bmcweb (upstream, C++, OpenBMC IBM p10bmc):"
curl -sk "$BMCWEB/redfish/v1" | python3 -m json.tool 2>/dev/null
echo ""
echo "bmcweb-ng (Rust rewrite — not yet running):"
NG_CODE=$(curl -s --max-time 3 -o /dev/null -w '%{http_code}' "$NG/redfish/v1" 2>/dev/null || echo "000")
if [ "$NG_CODE" = "200" ] || [ "$NG_CODE" = "401" ]; then
  curl -s "$NG/redfish/v1" | python3 -m json.tool 2>/dev/null
else
  echo "  [not running — HTTP $NG_CODE]"
fi

echo ""
echo "============================================================"
echo "  /redfish/v1/Systems"
echo "============================================================"
echo ""
echo "bmcweb:"
curl -sk $AUTH "$BMCWEB/redfish/v1/Systems" | python3 -m json.tool 2>/dev/null
echo ""
echo "bmcweb-ng:"
if [ "$NG_CODE" = "200" ] || [ "$NG_CODE" = "401" ]; then
  curl -s "$NG/redfish/v1/Systems" | python3 -m json.tool 2>/dev/null
else echo "  [not running]"; fi

echo ""
echo "============================================================"
echo "  /redfish/v1/Systems/system"
echo "============================================================"
echo ""
echo "bmcweb:"
curl -sk $AUTH "$BMCWEB/redfish/v1/Systems/system" | python3 -m json.tool 2>/dev/null | head -60
echo ""
echo "bmcweb-ng:"
if [ "$NG_CODE" = "200" ] || [ "$NG_CODE" = "401" ]; then
  curl -s "$NG/redfish/v1/Systems/system" | python3 -m json.tool 2>/dev/null | head -60
else echo "  [not running]"; fi

echo ""
echo "============================================================"
echo "  /redfish/v1/Chassis"
echo "============================================================"
echo ""
echo "bmcweb:"
curl -sk $AUTH "$BMCWEB/redfish/v1/Chassis" | python3 -m json.tool 2>/dev/null
echo ""
echo "bmcweb-ng:"
if [ "$NG_CODE" = "200" ] || [ "$NG_CODE" = "401" ]; then
  curl -s "$NG/redfish/v1/Chassis" | python3 -m json.tool 2>/dev/null
else echo "  [not running]"; fi

echo ""
echo "============================================================"
echo "  /redfish/v1/Managers"
echo "============================================================"
echo ""
echo "bmcweb:"
curl -sk $AUTH "$BMCWEB/redfish/v1/Managers" | python3 -m json.tool 2>/dev/null
echo ""
echo "bmcweb-ng:"
if [ "$NG_CODE" = "200" ] || [ "$NG_CODE" = "401" ]; then
  curl -s "$NG/redfish/v1/Managers" | python3 -m json.tool 2>/dev/null
else echo "  [not running]"; fi

echo ""
echo "============================================================"
echo "  /redfish/v1/Managers/bmc"
echo "============================================================"
echo ""
echo "bmcweb:"
curl -sk $AUTH "$BMCWEB/redfish/v1/Managers/bmc" | python3 -m json.tool 2>/dev/null | head -50
echo ""
echo "bmcweb-ng:"
if [ "$NG_CODE" = "200" ] || [ "$NG_CODE" = "401" ]; then
  curl -s "$NG/redfish/v1/Managers/bmc" | python3 -m json.tool 2>/dev/null | head -50
else echo "  [not running]"; fi

echo ""
echo "============================================================"
echo "  /redfish/v1/AccountService"
echo "============================================================"
echo ""
echo "bmcweb:"
curl -sk $AUTH "$BMCWEB/redfish/v1/AccountService" | python3 -m json.tool 2>/dev/null | head -40
echo ""
echo "bmcweb-ng:"
if [ "$NG_CODE" = "200" ] || [ "$NG_CODE" = "401" ]; then
  curl -s "$NG/redfish/v1/AccountService" | python3 -m json.tool 2>/dev/null | head -40
else echo "  [not running]"; fi

echo ""
echo "=== bmcweb-ng STATUS: HTTP $NG_CODE ==="
