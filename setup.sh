#!/usr/bin/env bash
set -e

git config core.hooksPath .githooks
echo "Git hooks configured. Pre-commit checks will run on each commit."
