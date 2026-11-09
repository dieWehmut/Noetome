$ErrorActionPreference='Continue'
Set-Location 'C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\workbench-gate'
$env:CARGO_TARGET_DIR='C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\target'
& 'C:\Users\Lyinn\.cargo\bin\cargo.exe' generate-lockfile --offline *> 'C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\workbench-gate.log'
$gen=$LASTEXITCODE
$one=-1;$two=-1
if($gen -eq 0){
& 'C:\Users\Lyinn\.cargo\bin\cargo.exe' test --locked workbench::tests::native_descendant_is_killed_after_the_root_exits_with_inherited_pipes -- --exact --nocapture *>> 'C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\workbench-gate.log'
$one=$LASTEXITCODE
& 'C:\Users\Lyinn\.cargo\bin\cargo.exe' test --locked workbench::tests::cancellation_interrupts_an_acknowledged_real_child_within_the_bound -- --exact --nocapture *>> 'C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\workbench-gate.log'
$two=$LASTEXITCODE
}
[IO.File]::WriteAllText('C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\workbench-gate-result.json',('{"lock_exit":'+$gen+',"root_exit_test_exit":'+$one+',"cancel_test_exit":'+$two+'}'))
if($gen -eq 0 -and $one -eq 0 -and $two -eq 0){exit 0}else{exit 1}
