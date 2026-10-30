#!/usr/bin/env bash
# Сборка APK: ядро на Rust → libbookworm.so (arm64), шрифты и корпус → assets, Gradle → APK.
# Результат: out/BookWorm.apk. Установка на телефон: adb install -r out/BookWorm.apk
# Для эмулятора: ABIS=arm64-v8a,x86_64 tools/build_apk.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export JAVA_HOME="${JAVA_HOME:-$(ls -d "$HOME"/.local/share/mise/installs/java/temurin-21* | head -1)}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK_VERSION="29.0.14206865"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$NDK_VERSION"
GRADLE="${GRADLE:-$(ls -d "$HOME"/.local/share/gradle-dist/gradle-*/bin/gradle | head -1)}"
APP="$ROOT/android/app/src/main"
ABIS="${ABIS:-arm64-v8a}"

if [ ! -d "$ANDROID_NDK_HOME" ]; then
    "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" --sdk_root="$ANDROID_HOME" \
        "ndk;$NDK_VERSION" "platforms;android-36" "build-tools;36.0.0" "platform-tools"
fi

echo "== ядро ($ABIS)"
rm -rf "$APP/jniLibs"
# Для телефона — без отладочной информации и со стрипом: библиотека в разы меньше.
CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_PROFILE_RELEASE_STRIP=true \
    cargo ndk --manifest-path "$ROOT/Cargo.toml" $(printf -- '-t %s ' ${ABIS//,/ }) --platform 31 -o "$APP/jniLibs" \
    build --release -p bookworm-android

echo "== шрифты"
# Встроенные гарнитуры (SPEC §4): Literata, PT Serif, Charis SIL, PT Sans — из fonts/, лицензия OFL.
mkdir -p "$APP/assets/fonts"
rm -f "$APP/assets/fonts/"*
cp "$ROOT"/fonts/*.ttf "$APP/assets/fonts/"

echo "== APK"
echo "sdk.dir=$ANDROID_HOME" > "$ROOT/android/local.properties"
"$GRADLE" -p "$ROOT/android" --no-daemon -q assembleRelease -Pabis="$ABIS"
mkdir -p "$ROOT/out"
cp "$ROOT/android/app/build/outputs/apk/release/app-release.apk" "$ROOT/out/BookWorm.apk"
ls -lh "$ROOT/out/BookWorm.apk"
