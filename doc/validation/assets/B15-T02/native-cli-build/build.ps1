$ErrorActionPreference='Continue'
Set-Location 'C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\source'
$env:CARGO_TARGET_DIR='C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\target'
& 'C:\Users\Lyinn\.cargo\bin\cargo.exe' build -p noetome-cli --locked *> 'C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\cargo-build.log'
$code=$LASTEXITCODE
[IO.File]::WriteAllText('C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\build-result.json',('{"exit_code":'+$code+'}'))
exit $code
