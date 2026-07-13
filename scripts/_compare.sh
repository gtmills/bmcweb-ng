#!/bin/bash
# Wait for bmcweb to be ready, then run a side-by-side comparison
# of upstream bmcweb (HTTPS 2443) vs bmcweb-ng (HTTP 8080).
# bmcweb-ng must already be running inside QEMU.

BMCWEB="https://127.0.0.1:2443"
NG="http://127.0.0.1:8080"
AUTH_BMCWEB="-u admin:0penBmc2!"

echo "=== Waiting for upstream bmcweb ==="
for i in $(seq 1 120); do
  CODE=$(curl -sk -o /dev/null -w '%{http_code}' "$BMCWEB/redfish/v1" 2>/dev/null)
  echo "  [${i}s] HTTP $CODE"
  if [ "$CODE" = "200" ] || [ "$CODE" = "401" ]; then
    echo "  bmcweb ready (HTTP $CODE)"
    break
  fi
  sleep 5
done

echo ""
echo "=== Comparison: GET /redfish/v1 ==="
echo "--- upstream bmcweb ---"
curl -sk "$BMCWEB/redfish/v1" | python3 -m json.tool 2>/dev/null || curl -sk "$BMCWEB/redfish/v1"
echo ""
echo "--- bmcweb-ng ---"
NG_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$NG/redfish/v1" 2>/dev/null)
if [ "$NG_CODE" = "200" ] || [ "$NG_CODE" = "401" ]; then
  curl -s "$NG/redfish/v1" | python3 -m json.tool 2>/dev/null || curl -s "$NG/redfish/v1"
else
  echo "  bmcweb-ng not running (HTTP $NG_CODE)"
fi

echo ""
echo "=== Comparison: GET /redfish/v1/Systems ==="
echo "--- upstream bmcweb ---"
curl -sk $AUTH_BMCWEB "$BMCWEB/redfish/v1/Systems" | python3 -m json.tool 2>/dev/null
echo ""
echo "--- bmcweb-ng ---"
if [ "$NG_CODE" != "000" ]; then
  curl -s "$NG/redfish/v1/Systems" | python3 -m json.tool 2>/dev/null || echo "(not available)"
else
  echo "(bmcweb-ng not running)"
fi

echo ""
echo "=== Comparison: GET /redfish/v1/Chassis ==="
echo "--- upstream bmcweb ---"
curl -sk $AUTH_BMCWEB "$BMCWEB/redfish/v1/Chassis" | python3 -m json.tool 2>/dev/null
echo ""
echo "--- bmcweb-ng ---"
if [ "$NG_CODE" != "000" ]; then
  curl -s "$NG/redfish/v1/Chassis" | python3 -m json.tool 2>/dev/null || echo "(not available)"
fi

echo ""
echo "=== Comparison: GET /redfish/v1/Managers ==="
echo "--- upstream bmcweb ---"
curl -sk $AUTH_BMCWEB "$BMCWEB/redfish/v1/Managers" | python3 -m json.tool 2>/dev/null
echo ""
echo "--- bmcweb-ng ---"
if [ "$NG_CODE" != "000" ]; then
  curl -s "$NG/redfish/v1/Managers" | python3 -m json.tool 2>/dev/null || echo "(not available)"
fi

echo ""
echo "=== Comparison: GET /redfish/v1/AccountService ==="
echo "--- upstream bmcweb ---"
curl -sk $AUTH_BMCWEB "$BMCWEB/redfish/v1/AccountService" | python3 -m json.tool 2>/dev/null | head -30
echo ""
echo "--- bmcweb-ng ---"
if [ "$NG_CODE" != "000" ]; then
  curl -s "$NG/redfish/v1/AccountService" | python3 -m json.tool 2>/dev/null | head -30 || echo "(not available)"
fi
