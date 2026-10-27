<#
  Test phần phân tích của scripts/measure.ps1 bằng CSV giả (không khởi động hub hay app, không động tới hệ thống).
  Chạy: pwsh scripts/test-measure.ps1
#>
$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot 'measure.ps1'
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("measure-test-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$failures = 0

function Write-Csv([string]$Name, [object[]]$Rows) {
    $path = Join-Path $work $Name
    $lines = @('ts,rss_kb,cpu_pct,priv_kb,phase')
    $i = 0
    foreach ($r in $Rows) {
        $lines += ('2026-10-27T10:00:{0:D2}.000Z,{1},{2},{3},{4}' -f $i, $r[0], $r[1], ($r[0] / 4), $r[2])
        $i++
    }
    Set-Content -Path $path -Value $lines -Encoding UTF8
    return $path
}

function Run-Analyze([string]$Csv, [hashtable]$EnvVars = @{}) {
    foreach ($k in $EnvVars.Keys) { Set-Item "Env:$k" $EnvVars[$k] }
    try {
        $out = & pwsh -NoProfile -File $script -Analyze $Csv 2>&1 | Out-String
        return [pscustomobject]@{ Code = $LASTEXITCODE; Out = $out.Trim() }
    }
    finally {
        foreach ($k in $EnvVars.Keys) { Remove-Item "Env:$k" -ErrorAction SilentlyContinue }
    }
}

function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
    if ($Ok) { Write-Host "ok   $Name" }
    else { Write-Host "FAIL $Name $Detail"; $script:failures++ }
}

# Hub đạt: idle 12 MB, đỉnh 30 MB, nghỉ 13 MB, CPU ≈ 0.
$good = @()
1..3 | ForEach-Object { $good += , @(12288, '0.00', 'idle') }
1..4 | ForEach-Object { $good += , @((30 * 1024), '1.50', 'load') }
1..3 | ForEach-Object { $good += , @((13 * 1024), '0.00', 'rest') }
$r = Run-Analyze (Write-Csv 'good.csv' $good) @{ MEASURE_INGEST_P99_MS = '0.93' }
Check 'hub đạt: PASS và mã thoát 0' ($r.Code -eq 0 -and $r.Out -match 'verdict=PASS' -and $r.Out -match 'target=hub') $r.Out
Check 'hub: số liệu trong dòng tóm tắt' ($r.Out -match 'idle_rss_mb=12\.0' -and $r.Out -match 'load_peak_rss_mb=30\.0' -and $r.Out -match 'rest_rss_mb=13\.0' -and $r.Out -match 'ingest_p99_ms=0\.93') $r.Out

# Hub vượt từng mục.
$cases = @(
    @{ Name = 'idle vượt 15 MB'; Rows = @(@((18 * 1024), '0.00', 'idle'), @((18 * 1024), '0.00', 'rest')); Want = 'idle_rss>15' },
    @{ Name = 'đỉnh vượt 40 MB'; Rows = @(@(12288, '0.00', 'idle'), @((45 * 1024), '2.00', 'load'), @(12288, '0.00', 'rest')); Want = 'load_peak>40' },
    @{ Name = 'CPU idle vượt 0.5%'; Rows = @(@(12288, '3.00', 'idle'), @(12288, '0.00', 'rest')); Want = 'idle_cpu>0.5' },
    @{ Name = 'rò rỉ sau khi nghỉ'; Rows = @(@(12288, '0.00', 'idle'), @((30 * 1024), '1.00', 'load'), @((25 * 1024), '0.00', 'rest')); Want = 'ro_ri>5' }
)
foreach ($c in $cases) {
    $r = Run-Analyze (Write-Csv 'case.csv' $c.Rows)
    Check "hub FAIL: $($c.Name)" ($r.Code -eq 1 -and $r.Out -match 'verdict=FAIL' -and $r.Out.Contains($c.Want)) $r.Out
}
$r = Run-Analyze (Write-Csv 'p99.csv' @(, @(12288, '0.00', 'idle'))) @{ MEASURE_INGEST_P99_MS = '9.5' }
Check 'hub FAIL: p99 vượt 5 ms' ($r.Code -eq 1 -and $r.Out.Contains('p99>5')) $r.Out
$r = Run-Analyze (Write-Csv 'noidle.csv' @(, @(12288, '0.00', 'load')))
Check 'hub FAIL: thiếu mẫu idle' ($r.Code -eq 1 -and $r.Out.Contains('khong_co_mau_idle')) $r.Out

# App đạt: tray 17 MB, popup mở 140 MB, đóng lại 18 MB, popup mở 250 ms.
$app = @()
1..3 | ForEach-Object { $app += , @((17 * 1024), '0.00', 'tray') }
1..2 | ForEach-Object { $app += , @((90 * 1024), '0.10', 'popup_cycle') }
1..3 | ForEach-Object { $app += , @((140 * 1024), '0.20', 'popup_open') }
1..3 | ForEach-Object { $app += , @((18 * 1024), '0.00', 'after_close') }
$appCsv = Write-Csv 'app.csv' $app
$r = Run-Analyze $appCsv @{ MEASURE_POPUP_READY_MS = '240,250,300' }
Check 'app đạt: PASS' ($r.Code -eq 0 -and $r.Out -match 'target=app' -and $r.Out -match 'verdict=PASS' -and $r.Out -match 'popup_ready_median_ms=250') $r.Out
$r = Run-Analyze $appCsv @{ MEASURE_POPUP_READY_MS = '380,420,450' }
Check 'app FAIL: popup mở chậm hơn 300 ms' ($r.Code -eq 1 -and $r.Out.Contains('popup_ready_median>300')) $r.Out
$r = Run-Analyze $appCsv
Check 'app FAIL: không đo được popup_ready' ($r.Code -eq 1 -and $r.Out.Contains('khong_do_duoc_popup_ready')) $r.Out
$big = @(@((70 * 1024), '0.00', 'tray'), @((200 * 1024), '0.00', 'popup_open'), @((65 * 1024), '0.00', 'after_close'))
$r = Run-Analyze (Write-Csv 'appbig.csv' $big) @{ MEASURE_POPUP_READY_MS = '200' }
Check 'app FAIL: tray, popup và sau đóng đều vượt' ($r.Code -eq 1 -and $r.Out.Contains('tray_rss>60') -and $r.Out.Contains('popup_rss>150') -and $r.Out.Contains('sau_dong_rss>60')) $r.Out

$r = Run-Analyze $appCsv @{ MEASURE_POPUP_READY_MS = '7,8,10'; MEASURE_PREWARM_DROPPED = '1' }
Check 'app prewarm: popup mở 7 ms và đã hủy popup bỏ dở' ($r.Code -eq 0 -and $r.Out -match 'popup_ready_median_ms=8' -and $r.Out.Contains('prewarm_dropped=1')) $r.Out
$r = Run-Analyze $appCsv @{ MEASURE_POPUP_READY_MS = '7,8,10'; MEASURE_PREWARM_DROPPED = '0' }
Check 'app FAIL: popup tạo sẵn không bị hủy khi rời icon' ($r.Code -eq 1 -and $r.Out.Contains('prewarm_khong_bi_huy')) $r.Out

Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
if ($failures -gt 0) { Write-Host "$failures kiểm tra lỗi"; exit 1 }
Write-Host 'tất cả kiểm tra đạt'
