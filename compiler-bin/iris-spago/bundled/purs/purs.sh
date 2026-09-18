#!/bin/sh
if [ "$#" -eq 1 ] && [ "$1" = "--version" ]; then
  printf '%s\n' '0.15.15'
  exit 0
fi
printf '%s\n' 'Iris compatibility shim only supports purs --version' >&2
exit 64
