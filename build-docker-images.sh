#!/usr/bin/env bash
#
# Build all of the docker images which are used for these experiements
docker build --file Dockerfile.alpine-custom-rust -t elastio:alpine-custom-rust .
docker build --file Dockerfile.alpine-official-rust -t elastio:alpine-official-rust .
docker build --file Dockerfile.debian-rust -t elastio:debian-rust .
docker build --file Dockerfile.debian-rust -t elastio:debian-static-libs .
