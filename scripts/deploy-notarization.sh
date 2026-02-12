#!/usr/bin/env bash
# Deploy the IOTA notarization Move package to testnet or mainnet.
# Usage: ./scripts/deploy-notarization.sh [testnet|mainnet]
# Requires: iota CLI, jq, git
set -euo pipefail

NETWORK="${1:-testnet}"

if [[ "$NETWORK" != "testnet" && "$NETWORK" != "mainnet" ]]; then
    echo "Usage: $0 [testnet|mainnet]"
    exit 1
fi

for cmd in iota jq git; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "Error: '$cmd' not found in PATH."
        exit 1
    fi
done

# Validate active environment matches requested network
ACTIVE_ENV=$(iota client active-env 2>/dev/null || true)
if [[ "$ACTIVE_ENV" != "$NETWORK" ]]; then
    echo "Error: active iota env is '$ACTIVE_ENV', expected '$NETWORK'."
    echo "Run: iota client switch --env $NETWORK"
    exit 1
fi

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Cloning notarization repo..."
git clone --depth 1 --branch v0.1 https://github.com/iotaledger/notarization.git "$TMPDIR/notarization"

echo "Publishing notarization Move package on $NETWORK..."
PUBLISH_OUTPUT=$(iota client publish "$TMPDIR/notarization/notarization-move/" --json --gas-budget 500000000)

PACKAGE_ID=$(echo "$PUBLISH_OUTPUT" | jq -r '.objectChanges[] | select(.type == "published") | .packageId')

if [[ -z "$PACKAGE_ID" || "$PACKAGE_ID" == "null" ]]; then
    echo "Error: could not extract package ID from publish output."
    echo "$PUBLISH_OUTPUT" | jq .
    exit 1
fi

echo ""
echo "Package ID: $PACKAGE_ID"
echo ""
echo "export IOTA_NOTARIZATION_PKG_ID=$PACKAGE_ID"
