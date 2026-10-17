#!/usr/bin/env bash
# Create (or validate) the self-signed Android release keystore.
#
# Resolution order:
#   1. ANDROID_KEYSTORE_BASE64   -> decode a pinned keystore (recommended)
#   2. android/keystore/release.p12 already present and opens with the password
#   3. generate a new RSA-4096 self-signed PKCS#12 certificate
#
# The keystore and keystore.properties are written into android/ (gitignored).
# Nothing secret is printed. Only the SHA-256 fingerprint is emitted.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KEYSTORE_DIR="$ROOT/android/keystore"
KEYSTORE="$KEYSTORE_DIR/release.p12"
PROPERTIES="$ROOT/android/keystore.properties"
FINGERPRINT_FILE="$ROOT/android-signing-fingerprint.txt"
DEFAULT_PASSWORD="rustvnc-selfsigned-v1"

store_pass="${ANDROID_KEYSTORE_PASSWORD:-$DEFAULT_PASSWORD}"
key_alias="${ANDROID_KEY_ALIAS:-rustvnc}"
# PKCS#12 requires identical store/key passwords; a pinned JKS may differ.
key_pass="${ANDROID_KEY_PASSWORD:-$store_pass}"
store_type="${ANDROID_KEYSTORE_TYPE:-PKCS12}"

# PKCS#12 requires the key password to match the store password. A pinned JKS
# (ANDROID_KEYSTORE_BASE64) may legitimately use a different key password.
if [ -z "${ANDROID_KEYSTORE_BASE64:-}" ] && [ "$store_type" = "PKCS12" ]; then
  key_pass="$store_pass"
fi

if [ "$store_pass" = "$DEFAULT_PASSWORD" ]; then
  echo "::warning::ANDROID_KEYSTORE_PASSWORD is not set; using the public default password. Pin a keystore via secrets for production releases."
fi

mkdir -p "$KEYSTORE_DIR"

validate() {
  keytool -list -keystore "$1" -storepass "$store_pass" >/dev/null 2>&1
}

if [ -n "${ANDROID_KEYSTORE_BASE64:-}" ]; then
  echo "Restoring Android keystore from ANDROID_KEYSTORE_BASE64"
  printf '%s' "$ANDROID_KEYSTORE_BASE64" | base64 -d > "$KEYSTORE"
  if ! validate "$KEYSTORE"; then
    echo "::error::The pinned keystore cannot be opened with the configured password/alias."
    exit 1
  fi
elif [ -f "$KEYSTORE" ] && validate "$KEYSTORE"; then
  echo "Using the existing cached Android keystore"
else
  echo "Generating a new self-signed Android release certificate"
  rm -f "$KEYSTORE"
  keytool -genkeypair -v \
    -keystore "$KEYSTORE" \
    -storetype PKCS12 \
    -alias "$key_alias" \
    -keyalg RSA \
    -keysize 4096 \
    -validity 10000 \
    -storepass "$store_pass" \
    -keypass "$key_pass" \
    -dname "CN=RustVNC, OU=Self-Signed, O=RustVNC, L=Local, ST=Local, C=CN" \
    >/dev/null
fi

cat > "$PROPERTIES" <<EOF
storeFile=keystore/release.p12
storePassword=$store_pass
keyAlias=$key_alias
keyPassword=$key_pass
storeType=$store_type
EOF

fingerprint="$(keytool -list -v -keystore "$KEYSTORE" -storepass "$store_pass" -alias "$key_alias" 2>/dev/null | sed -n 's/.*SHA256: //p' | head -n1)"
if [ -z "$fingerprint" ]; then
  echo "::error::Could not read the SHA-256 fingerprint from the keystore."
  exit 1
fi
printf '%s\n' "$fingerprint" > "$FINGERPRINT_FILE"
echo "Android signing certificate SHA-256: $fingerprint"

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### Android signing certificate"
    echo
    echo "- SHA-256: \`${fingerprint}\`"
    echo "- keystore: \`android/keystore/release.p12\` (not committed, not uploaded)"
  } >> "$GITHUB_STEP_SUMMARY"
fi
