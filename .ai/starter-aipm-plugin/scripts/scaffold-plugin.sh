#!/usr/bin/env bash
set -euo pipefail
# Scaffold a new AI plugin using the aipm CLI.
# Usage: bash scaffold-plugin.sh <plugin-name> [claude|copilot|both]
aipm make plugin --name "${1:?Plugin name required}" --engine "${2:-claude}" --feature skill -y
