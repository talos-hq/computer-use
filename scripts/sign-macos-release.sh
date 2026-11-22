#!/usr/bin/env bash
set -euo pipefail
app=${1:?missing app}
entitlements=${2:?missing entitlements}
: "${APPLE_SIGNING_IDENTITY:?}" "${APPLE_TEAM:?}" "${APPLE_NOTARY_USER:?}" "${APPLE_NOTARY_PASSWORD:?}"
main="$app/Contents/MacOS/computer-use"
sign=(codesign --force --timestamp --options runtime --sign "$APPLE_SIGNING_IDENTITY")
while IFS= read -r -d '' binary; do
    if [[ "$binary" != "$main" && $(file -b "$binary") == *Mach-O* ]]; then
        "${sign[@]}" "$binary"
    fi
done < <(find "$app/Contents" -depth -type f -print0)
"${sign[@]}" --identifier com.taloshq.computer-use --entitlements "$entitlements" "$main"
"${sign[@]}" --entitlements "$entitlements" "$app"
codesign --verify --deep --strict --verbose=2 "$app"
zip="$RUNNER_TEMP/notary.zip"
ditto -c -k --keepParent "$app" "$zip"
auth=(--apple-id "$APPLE_NOTARY_USER" --password "$APPLE_NOTARY_PASSWORD" --team-id "$APPLE_TEAM")
result="$RUNNER_TEMP/notary.json"
if ! xcrun notarytool submit "$zip" "${auth[@]}" --wait --output-format json > "$result" ||
   [[ $(plutil -extract status raw "$result") != Accepted ]]; then
    if id=$(plutil -extract id raw "$result"); then
        xcrun notarytool log "$id" "${auth[@]}"
    fi
    exit 1
fi
xcrun stapler staple "$app"
xcrun stapler validate "$app"
spctl -a -vv -t exec "$app"
