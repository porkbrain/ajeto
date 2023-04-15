#!/bin/bash

set -e

cargo build --release

# these will written into with docker root
mkdir -p .tmp/db
mkdir -p .tmp/home

docker-compose \
    -f tests/assets/docker-compose.yml \
    --project-directory . \
    up --build
