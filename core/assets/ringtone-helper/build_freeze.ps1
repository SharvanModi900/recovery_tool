$ANDROID_JAR = "C:\Users\Sharvan Modi\AppData\Local\Android\Sdk\platforms\android-33\android.jar"
$SRC = "D:\recovery-tool\core\assets\ringtone-helper\src\com\recoverkit\audio\DeviceFreezer.java"
$OUT = "D:\recovery-tool\core\assets\ringtone-helper\classes_freeze"
$DEX = "D:\recovery-tool\core\assets\ringtone-helper\dex_freeze"
$D8 = "C:\Users\Sharvan Modi\AppData\Local\Android\Sdk\build-tools\34.0.0\d8.bat"

if (Test-Path $OUT) { Remove-Item -Recurse -Force $OUT }
New-Item -ItemType Directory -Force -Path $OUT | Out-Null

Write-Host "Compiling DeviceFreezer.java..."
& javac -encoding UTF-8 -source 1.8 -target 1.8 -classpath $ANDROID_JAR -d $OUT $SRC 2>&1
Write-Host "javac exit: $LASTEXITCODE"

Get-ChildItem $OUT -Recurse -Filter "*.class" | ForEach-Object { Write-Host "  class: $($_.FullName) ($($_.Length) bytes)" }

if (Test-Path $DEX) { Remove-Item -Recurse -Force $DEX }
New-Item -ItemType Directory -Force -Path $DEX | Out-Null

Write-Host "Generating DEX..."
& $D8 --output $DEX --min-api 21 "$OUT\com\recoverkit\audio\DeviceFreezer.class" 2>&1
Write-Host "d8 exit: $LASTEXITCODE"

Get-ChildItem $DEX -Filter "*.dex" | ForEach-Object { Write-Host "  DEX: $($_.FullName) ($($_.Length) bytes)" }
