#!/usr/bin/env bash

WSLGIT_BINARY=target/release/wslgit.exe
OUTPUT_DIR=release/wslgit
OUTPUT_CMD_DIR=$OUTPUT_DIR/cmd

[[ ! -f "$WSLGIT_BINARY" ]] && echo "Release not built!" && exit 1

rm -rf release/* || exit 1
mkdir -p $OUTPUT_CMD_DIR || exit 1

cp "$WSLGIT_BINARY" "$OUTPUT_CMD_DIR" || exit 1
cp resources/Fork.RI "$OUTPUT_CMD_DIR" || exit 1
cp resources/install.bat "$OUTPUT_DIR" || exit 1

cd release && zip -r wslgit.zip ./*
