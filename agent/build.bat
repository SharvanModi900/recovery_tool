@echo off
REM Build script for RecoverKit Remote Agent APK
REM Requires: Android SDK (platform-tools + build-tools + platform android-33)

set ANDROID_SDK=C:\Users\Sharvan Modi\AppData\Local\Android\Sdk
set ANDROID_JAR=%ANDROID_SDK%\platforms\android-33\android.jar
set BUILD_TOOLS=%ANDROID_SDK%\build-tools\34.0.0
set SRC_DIR=D:\recovery-tool\agent\src
set OUT_DIR=D:\recovery-tool\agent\classes
set DEX_OUT=D:\recovery-tool\agent\dex
set APK_UNSIGNED=D:\recovery-tool\agent\recoverkit-agent-unsigned.apk
set APK_OUT=D:\recovery-tool\agent\recoverkit-agent.apk
set KEYSTORE=D:\recovery-tool\agent\debug.keystore

if exist "%OUT_DIR%" rmdir /s /q "%OUT_DIR%"
mkdir "%OUT_DIR%"
if exist "%DEX_OUT%" rmdir /s /q "%DEX_OUT%"
mkdir "%DEX_OUT%"

echo [1/7] Compiling Java...
javac -source 1.8 -target 1.8 -classpath "%ANDROID_JAR%" -d "%OUT_DIR%" ^
  "%SRC_DIR%\com\recoverkit\agent\RemoteAgent.java" 2>&1
echo javac exit: %ERRORLEVEL%

echo [2/7] Generating DEX...
"%BUILD_TOOLS%\d8.bat" --output "%DEX_OUT%" ^
  "%OUT_DIR%\com\recoverkit\agent\RemoteAgent.class" 2>&1
echo d8 exit: %ERRORLEVEL%

echo [3/7] Packaging APK with aapt2...
"%BUILD_TOOLS%\aapt2.exe" link ^
  -I "%ANDROID_JAR%" ^
  --manifest "D:\recovery-tool\agent\AndroidManifest.xml" ^
  -o "%APK_UNSIGNED%" 2>&1
echo aapt2 link exit: %ERRORLEVEL%

echo [4/7] Adding DEX to APK...
cd "%DEX_OUT%"
jar uf "%APK_UNSIGNED%" classes.dex 2>&1
cd D:\recovery-tool\agent

echo [5/7] Aligning APK...
"%BUILD_TOOLS%\zipalign.exe" -f 4 "%APK_UNSIGNED%" "%APK_OUT%" 2>&1
echo zipalign exit: %ERRORLEVEL%

echo [6/7] Generating keystore (if needed)...
if not exist "%KEYSTORE%" (
  keytool -genkey -v -keystore "%KEYSTORE%" -alias recoverkit ^
    -keyalg RSA -keysize 2048 -validity 10000 ^
    -storepass android -keypass android -dname "CN=RecoverKit" 2>&1
)

echo [7/7] Signing APK...
"%BUILD_TOOLS%\apksigner.bat" sign ^
  --ks "%KEYSTORE%" ^
  --ks-key-alias recoverkit ^
  --ks-pass pass:android ^
  --key-pass pass:android ^
  "%APK_OUT%" 2>&1
echo apksigner exit: %ERRORLEVEL%

del "%APK_UNSIGNED%" 2>nul

for %%F in ("%APK_OUT%") do echo APK size: %%~zF bytes
echo.
echo Build complete: %APK_OUT%
echo Install: adb install %APK_OUT%
