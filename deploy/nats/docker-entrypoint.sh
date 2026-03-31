#!/bin/sh
set -e
# If operator JWT and resolver directory exist (from `nsc`), use JWT config; otherwise anonymous dev mode.
if [ -f /etc/nats/keys/operator.jwt ] && [ -d /etc/nats/keys/jwt ] && [ "$(find /etc/nats/keys/jwt -name '*.jwt' 2>/dev/null | wc -l)" -ge 1 ]; then
  exec nats-server -c /etc/nats/nats-server-jwt.conf
fi
exec nats-server -c /etc/nats/nats-server.conf
