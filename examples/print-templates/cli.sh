#!/bin/sh
set -eu
if [ "$#" -ne 3 ]; then
    echo "Usage: sh cli.sh TEMPLATE ORDER.json OUTPUT_DIRECTORY" >&2
    exit 2
fi
mkdir -p "$3"
labelize template fill "$1" --data "$2" -o "$3/order.${1##*.}"
labelize template render "$1" --data "$2" --dpmm 8 --width 100 --height 50 -o "$3/order.png"
