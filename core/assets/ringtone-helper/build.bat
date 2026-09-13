@echo off
set ANDROID_JAR=C:\Users\Sharvan Modi\AppData\Local\Android\Sdk\platforms\android-33\android.jar
set SRC_DIR=D:\recovery-tool\core\assets\ringtone-helper\src
set OUT_DIR=D:\recovery-tool\core\assets\ringtone-helper\classes
set DEX_OUT=D:\recovery-tool\core\assets\ringtone-helper\dex

if exist "%OUT_DIR%" rmdir /s /q "%OUT_DIR%"
mkdir "%OUT_DIR%"

echo Compiling Java...
javac -source 1.8 -target 1.8 -classpath "%ANDROID_JAR%" -d "%OUT_DIR%" ^
  "%SRC_DIR%\com\recoverkit\audio\DeviceLocker.java" ^
  "%SRC_DIR%\com\recoverkit\audio\SilentPlayer.java" 2>&1
echo javac exit: %ERRORLEVEL%

echo Generating DEX...
"C:\Users\Sharvan Modi\AppData\Local\Android\Sdk\build-tools\34.0.0\d8.bat" --output "%DEX_OUT%" ^
  "%OUT_DIR%\com\recoverkit\audio\DeviceLocker.class" ^
  "%OUT_DIR%\com\recoverkit\audio\SilentPlayer.class" 2>&1
echo d8 exit: %ERRORLEVEL%

for %%F in ("%DEX_OUT%\classes.dex") do echo DEX size: %%~zF bytes
