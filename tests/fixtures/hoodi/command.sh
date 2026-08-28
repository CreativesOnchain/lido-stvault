#!/usr/bin/env bash
# Reproducible verification command for Hoodi testnet consolidations
stvault-receipt \
  --manifest ./tests/fixtures/hoodi/manifest.json \
  --el-tx 0x4a2a33f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c4 \
  --el-rpc https://rpc.hoodi.ethpandaops.io \
  --cl-beacon-api https://bn.hoodi.ethpandaops.io \
  --dashboard 0x1234567890123456789012345678901234567890 \
  --output-dir ./output
