#!/usr/bin/env bash
set -euo pipefail

CLERK_JWKS_HOST="${CLERK_JWKS_HOST:-clerk.lqcouncil.com}"
CLERK_ACCOUNTS_HOST="${CLERK_ACCOUNTS_HOST:-accounts.lqcouncil.com}"
CLERK_JWKS_URL="${CLERK_JWKS_URL:-https://${CLERK_JWKS_HOST}/.well-known/jwks.json}"
CLERK_SIGN_IN_URL="${CLERK_SIGN_IN_URL:-https://${CLERK_ACCOUNTS_HOST}/sign-in}"

check_dns() {
  local host="$1"
  if command -v nslookup >/dev/null 2>&1; then
    nslookup "$host" >/dev/null
    return
  fi
  if command -v dig >/dev/null 2>&1; then
    dig +short "$host" | grep -q "."
    return
  fi
  if command -v getent >/dev/null 2>&1; then
    getent hosts "$host" >/dev/null
    return
  fi
  echo "No DNS tool available (need nslookup, dig, or getent)." >&2
  exit 2
}

check_https_ok() {
  local url="$1"
  local status
  status="$(curl -sSL -o /dev/null -w "%{http_code}" --max-time 15 "$url")"
  case "$status" in
    200|301|302|303|307|308|401|403|405) ;;
    *)
      echo "HTTPS probe failed for $url (status=$status)" >&2
      exit 1
      ;;
  esac
}

echo "Checking DNS for $CLERK_JWKS_HOST and $CLERK_ACCOUNTS_HOST..."
check_dns "$CLERK_JWKS_HOST"
check_dns "$CLERK_ACCOUNTS_HOST"

echo "Checking HTTPS for Clerk JWKS and sign-in endpoints..."
check_https_ok "$CLERK_JWKS_URL"
check_https_ok "$CLERK_SIGN_IN_URL"

echo "Auth provider health checks passed."
