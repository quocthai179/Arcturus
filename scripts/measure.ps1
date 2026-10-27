<#
.SYNOPSIS
  Đo RAM/CPU/độ trễ của hub và app Arcturus theo kịch bản chuẩn của docs/BUDGET.md (docs/PLAN.md T16 · ARC-37).

.DESCRIPTION
  -Target hub (mặc định): khởi động hub bản release trên cổng riêng với CSDL tạm, chờ -IdleSec giây (idle), phát lại
  fixture bằng tools/replay (-Sessions phiên, -Rate yêu cầu/giây, -LoadSec giây), nghỉ -RestSec giây (kiểm rò rỉ).
  -Target app: khởi động app bản release (hub do app tự khởi động) ở chế độ đo popup (ARCTURUS_MEASURE_POPUP): đo tray-only,
  mở/đóng popup -Popups lần (đo thời gian mở), giữ popup mở rồi đóng, đo lại.
  -Analyze <csv>: chỉ phân tích một tệp CSV đã có (dùng cho test và cho agent budget-guard).

  Mỗi mẫu một dòng CSV: ts,rss_kb,cpu_pct,priv_kb,phase (ba cột đầu theo PLAN; thêm bộ nhớ riêng và tên pha).
  rss_kb là working set (tương đương RSS của Linux); cpu_pct tính theo một nhân. In đúng một dòng tóm tắt bắt đầu bằng
  "measure " (key=value) và kết thúc bằng verdict=PASS|FAIL; mã thoát 1 khi FAIL.

.EXAMPLE
  pwsh scripts/measure.ps1                       # hub: idle 60 s, tải 120 s, nghỉ 60 s
  pwsh scripts/measure.ps1 -Target app           # app: tray-only, popup
  pwsh scripts/measure.ps1 -Analyze out/hub.csv  # chỉ phân tích
#>
[CmdletBinding()]
param(
    [ValidateSet('hub', 'app')][string]$Target = 'hub',
    [string]$Analyze,
    [string]$HubBin,
    [string]$ReplayBin,
    [string]$AppBin,
    [string]$Fixtures = 'fixtures/hooks',
    [int]$IdleSec = 60,
    [int]$LoadSec = 120,
    [int]$RestSec = 60,
    [int]$Sessions = 20,
    [double]$Rate = 50,
    [int]$IntervalSec = 5,
    [int]$Port = 7899,
    [int]$Popups = 5,
    [int]$HoldSec = 20,
    [int]$DelaySec = 30,
    [int]$HoverMs = 0,
    [string]$OutDir,
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'

# Ngân sách (docs/BUDGET.md). Đổi ở đây phải đổi cả bảng "Mục tiêu".
$Budget = @{
    HubIdleMB      = 15.0   # hub, 0 phiên
    HubLoadMB      = 40.0   # hub, 20 phiên hoạt động
    AppLiteMB      = 60.0   # app: tray + popup đóng (app và webview, không tính hub)
    AppPopupMB     = 150.0  # app: popup mở, tổng
    IdleCpuPct     = 0.5    # "≈ 0%" khi rảnh
    P99IngestMs    = 5.0
    PopupReadyMs   = 300.0  # tiêu chí T12: popup mở ≤ 300 ms
    LeakToleranceMB = 5.0   # nghỉ xong, RSS không được cao hơn idle quá chừng này
}

function Get-Percentile([double[]]$Values, [double]$P) {
    if (-not $Values -or $Values.Count -eq 0) { return 0.0 }
    $sorted = $Values | Sort-Object
    $rank = [math]::Ceiling($P / 100.0 * $sorted.Count)
    $idx = [math]::Min([math]::Max($rank, 1), $sorted.Count) - 1
    return [double]$sorted[$idx]
}

function Get-Avg([double[]]$Values) {
    if (-not $Values -or $Values.Count -eq 0) { return 0.0 }
    return ($Values | Measure-Object -Average).Average
}

function Get-Max([double[]]$Values) {
    if (-not $Values -or $Values.Count -eq 0) { return 0.0 }
    return ($Values | Measure-Object -Maximum).Maximum
}

function Format-Num([double]$V, [int]$Digits = 2) {
    return [string]::Format([System.Globalization.CultureInfo]::InvariantCulture, "{0:F$Digits}", $V)
}

# ---------------------------------------------------------------------------------------------------------------
# Phân tích CSV → dòng tóm tắt (thuần, test được)
# ---------------------------------------------------------------------------------------------------------------

function Read-Samples([string]$Path) {
    $rows = Import-Csv -Path $Path
    foreach ($r in $rows) {
        [pscustomobject]@{
            Ts    = $r.ts
            RssMB = [double]$r.rss_kb / 1024.0
            Cpu   = [double]::Parse($r.cpu_pct, [System.Globalization.CultureInfo]::InvariantCulture)
            Phase = $r.phase
        }
    }
}

function Select-Phase($Samples, [string[]]$Names) {
    return @($Samples | Where-Object { $Names -contains $_.Phase })
}

# Hub: idle, load, rest. $IngestP99 và $Extra do người gọi cấp (p99 lấy từ đầu ra của replay).
function Get-HubSummary($Samples, [double]$IngestP99, [string]$Commit) {
    $idle = @(Select-Phase $Samples @('idle'))
    $load = @(Select-Phase $Samples @('load'))
    $rest = @(Select-Phase $Samples @('rest'))
    $idleRss = Get-Avg ($idle | ForEach-Object { $_.RssMB })
    $loadPeak = Get-Max ($load | ForEach-Object { $_.RssMB })
    $restRss = Get-Avg ($rest | ForEach-Object { $_.RssMB })
    $idleCpu = Get-Avg (($idle + $rest) | ForEach-Object { $_.Cpu })
    $loadCpu = Get-Avg ($load | ForEach-Object { $_.Cpu })
    $fail = @()
    if ($idle.Count -eq 0) { $fail += 'khong_co_mau_idle' }
    if ($idleRss -gt $Budget.HubIdleMB) { $fail += "idle_rss>$($Budget.HubIdleMB)MB" }
    if ($load.Count -gt 0 -and $loadPeak -gt $Budget.HubLoadMB) { $fail += "load_peak>$($Budget.HubLoadMB)MB" }
    if ($idleCpu -gt $Budget.IdleCpuPct) { $fail += "idle_cpu>$($Budget.IdleCpuPct)%" }
    if ($rest.Count -gt 0 -and $restRss -gt ($idleRss + $Budget.LeakToleranceMB)) { $fail += "ro_ri>$($Budget.LeakToleranceMB)MB" }
    if ($IngestP99 -gt $Budget.P99IngestMs) { $fail += "p99>$($Budget.P99IngestMs)ms" }
    $verdict = if ($fail.Count -eq 0) { 'PASS' } else { 'FAIL' }
    $reasons = if ($fail.Count -eq 0) { '-' } else { $fail -join ',' }
    return ("measure target=hub commit={0} idle_rss_mb={1} idle_cpu_pct={2} load_peak_rss_mb={3} load_cpu_avg_pct={4} rest_rss_mb={5} ingest_p99_ms={6} reasons={7} verdict={8}" -f `
        $Commit, (Format-Num $idleRss 1), (Format-Num $idleCpu), (Format-Num $loadPeak 1), (Format-Num $loadCpu), (Format-Num $restRss 1), (Format-Num $IngestP99), $reasons, $verdict)
}

# App: tray, popup_cycle, popup_open, after_close. $ReadyMs: thời gian mở popup (ms) của từng lần.
function Get-AppSummary($Samples, [double[]]$ReadyMs, [string]$Commit) {
    $tray = @(Select-Phase $Samples @('tray'))
    $open = @(Select-Phase $Samples @('popup_open'))
    $after = @(Select-Phase $Samples @('after_close'))
    $trayRss = Get-Avg ($tray | ForEach-Object { $_.RssMB })
    $trayCpu = Get-Avg ($tray | ForEach-Object { $_.Cpu })
    $openPeak = Get-Max ($open | ForEach-Object { $_.RssMB })
    $openCpu = Get-Avg ($open | ForEach-Object { $_.Cpu })
    $afterRss = Get-Avg ($after | ForEach-Object { $_.RssMB })
    $afterCpu = Get-Avg ($after | ForEach-Object { $_.Cpu })
    $readyMed = Get-Percentile $ReadyMs 50
    $readyMax = Get-Max $ReadyMs
    $fail = @()
    if ($tray.Count -eq 0) { $fail += 'khong_co_mau_tray' }
    if ($trayRss -gt $Budget.AppLiteMB) { $fail += "tray_rss>$($Budget.AppLiteMB)MB" }
    if ($trayCpu -gt $Budget.IdleCpuPct) { $fail += "tray_cpu>$($Budget.IdleCpuPct)%" }
    if ($open.Count -gt 0 -and $openPeak -gt $Budget.AppPopupMB) { $fail += "popup_rss>$($Budget.AppPopupMB)MB" }
    if ($after.Count -gt 0 -and $afterRss -gt $Budget.AppLiteMB) { $fail += "sau_dong_rss>$($Budget.AppLiteMB)MB" }
    if ($ReadyMs.Count -eq 0) { $fail += 'khong_do_duoc_popup_ready' }
    elseif ($readyMed -gt $Budget.PopupReadyMs) { $fail += "popup_ready_median>$($Budget.PopupReadyMs)ms" }
    $verdict = if ($fail.Count -eq 0) { 'PASS' } else { 'FAIL' }
    $reasons = if ($fail.Count -eq 0) { '-' } else { $fail -join ',' }
    return ("measure target=app commit={0} tray_rss_mb={1} tray_cpu_pct={2} popup_open_rss_mb={3} popup_open_cpu_pct={4} after_close_rss_mb={5} after_close_cpu_pct={6} popup_ready_median_ms={7} popup_ready_max_ms={8} popup_runs={9} reasons={10} verdict={11}" -f `
        $Commit, (Format-Num $trayRss 1), (Format-Num $trayCpu), (Format-Num $openPeak 1), (Format-Num $openCpu), (Format-Num $afterRss 1), (Format-Num $afterCpu), (Format-Num $readyMed 0), (Format-Num $readyMax 0), $ReadyMs.Count, $reasons, $verdict)
}

if ($Analyze) {
    $samples = @(Read-Samples $Analyze)
    $isApp = @($samples | Where-Object { $_.Phase -in @('tray', 'popup_open', 'after_close') }).Count -gt 0
    # Chỉ phân tích CSV: p99/popup_ready lấy từ tham số môi trường nếu có (xem README đầu tệp).
    $p99 = if ($env:MEASURE_INGEST_P99_MS) { [double]::Parse($env:MEASURE_INGEST_P99_MS, [System.Globalization.CultureInfo]::InvariantCulture) } else { 0.0 }
    $ready = @()
    if ($env:MEASURE_POPUP_READY_MS) { $ready = @($env:MEASURE_POPUP_READY_MS.Split(',') | ForEach-Object { [double]::Parse($_, [System.Globalization.CultureInfo]::InvariantCulture) }) }
    $line = if ($isApp) { Get-AppSummary $samples $ready 'analyze' } else { Get-HubSummary $samples $p99 'analyze' }
    Write-Output $line
    if ($line -match 'verdict=FAIL') { exit 1 }
    exit 0
}

# ---------------------------------------------------------------------------------------------------------------
# Lấy mẫu
# ---------------------------------------------------------------------------------------------------------------

function Get-RepoRoot { return (Resolve-Path (Join-Path $PSScriptRoot '..')).Path }

function Get-Descendants([int]$RootId, [string[]]$ExcludeNames) {
    # Hub: chỉ đo chính tiến trình hub. Con của nó (conhost của cửa sổ ẩn, `claude agents --json` của reconcile mỗi 30 giây)
    # là chương trình khác, không tính vào ngân sách của hub.
    if ($ExcludeNames -contains '*') { return @($RootId) }
    $all = Get-CimInstance Win32_Process
    $ids = New-Object System.Collections.Generic.List[int]
    $ids.Add($RootId)
    $queue = New-Object System.Collections.Generic.Queue[int]
    $queue.Enqueue($RootId)
    while ($queue.Count -gt 0) {
        $p = $queue.Dequeue()
        foreach ($c in ($all | Where-Object { $_.ParentProcessId -eq $p })) {
            if ($ExcludeNames -contains ($c.Name -replace '\.exe$', '')) { continue }
            $ids.Add([int]$c.ProcessId)
            $queue.Enqueue([int]$c.ProcessId)
        }
    }
    return $ids
}

# Một mẫu: tổng working set, bộ nhớ riêng (KB) và CPU (giây) của cây tiến trình.
function Get-TreeSample([int]$RootId, [string[]]$ExcludeNames) {
    $ws = 0.0; $priv = 0.0; $cpu = 0.0
    foreach ($id in (Get-Descendants $RootId $ExcludeNames)) {
        $p = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($null -eq $p) { continue }
        $ws += $p.WorkingSet64
        $priv += $p.PrivateMemorySize64
        if ($null -ne $p.CPU) { $cpu += $p.CPU }
    }
    return [pscustomobject]@{ WsKb = [math]::Round($ws / 1KB); PrivKb = [math]::Round($priv / 1KB); CpuSec = $cpu }
}

$script:Rows = New-Object System.Collections.Generic.List[object]
$script:Prev = $null

function Add-Sample([int]$RootId, [string[]]$ExcludeNames, [string]$Phase) {
    $s = Get-TreeSample $RootId $ExcludeNames
    $now = Get-Date
    $cpuPct = 0.0
    if ($null -ne $script:Prev) {
        $dt = ($now - $script:Prev.At).TotalSeconds
        if ($dt -gt 0) { $cpuPct = [math]::Max(0.0, ($s.CpuSec - $script:Prev.CpuSec) / $dt * 100.0) }
    }
    $script:Prev = [pscustomobject]@{ At = $now; CpuSec = $s.CpuSec }
    $row = [pscustomobject]@{
        ts      = $now.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        rss_kb  = $s.WsKb
        cpu_pct = (Format-Num $cpuPct)
        priv_kb = $s.PrivKb
        phase   = $Phase
    }
    $script:Rows.Add($row)
    return $row
}

function Wait-Sampling([int]$RootId, [string[]]$ExcludeNames, [string]$Phase, [int]$Seconds) {
    $end = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $end) {
        Start-Sleep -Seconds $IntervalSec
        $r = Add-Sample $RootId $ExcludeNames $Phase
        Write-Host ("[{0}] {1,-6} rss={2:N1} MB cpu={3}%" -f $r.ts.Substring(11, 8), $Phase, ($r.rss_kb / 1024.0), $r.cpu_pct)
    }
}

function Save-Csv([string]$Path) {
    $script:Rows | Export-Csv -Path $Path -NoTypeInformation -Encoding UTF8
}

function Get-Commit {
    try { return (git -C (Get-RepoRoot) rev-parse --short HEAD).Trim() } catch { return 'unknown' }
}

function Resolve-Bin([string]$Given, [string]$Default, [scriptblock]$Build) {
    $path = if ($Given) { $Given } else { Join-Path (Get-RepoRoot) $Default }
    if (-not (Test-Path $path)) {
        if ($NoBuild) { throw "Không thấy $path (đã bật -NoBuild)" }
        Write-Host "Build $path ..."
        & $Build
        if (-not (Test-Path $path)) { throw "Build xong nhưng vẫn không thấy $path" }
    }
    return (Resolve-Path $path).Path
}

function New-Token { return -join ((1..32) | ForEach-Object { '{0:x2}' -f (Get-Random -Maximum 256) }) }

# ---------------------------------------------------------------------------------------------------------------
# Kịch bản
# ---------------------------------------------------------------------------------------------------------------

$root = Get-RepoRoot
if (-not $OutDir) { $OutDir = Join-Path $root 'target/measure' }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("arcturus-measure-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$commit = Get-Commit
$hookToken = if ($env:ARCTURUS_TOKEN) { $env:ARCTURUS_TOKEN } else { New-Token }
$savedToken = $env:ARCTURUS_TOKEN
$env:ARCTURUS_TOKEN = $hookToken
$started = @()

try {
    $hubExe = Resolve-Bin $HubBin 'target/release/hub.exe' { cargo build --release -p hub --manifest-path (Join-Path $root 'Cargo.toml') }

    if ($Target -eq 'hub') {
        $replayExe = Resolve-Bin $ReplayBin 'target/release/replay.exe' { cargo build --release -p replay --manifest-path (Join-Path $root 'Cargo.toml') }
        $fix = if ([System.IO.Path]::IsPathRooted($Fixtures)) { $Fixtures } else { Join-Path $root $Fixtures }
        $db = Join-Path $work 'arcturus.db'
        $hubErr = Join-Path $work 'hub.err'
        $hub = Start-Process -FilePath $hubExe -ArgumentList @('--bind', "127.0.0.1:$Port", '--db', $db, '--ui-token-file', (Join-Path $work 'ui-token')) `
            -PassThru -WindowStyle Hidden -RedirectStandardError $hubErr -RedirectStandardOutput (Join-Path $work 'hub.out')
        $started += $hub
        Start-Sleep -Seconds 2
        if ($hub.HasExited) { throw "Hub thoát sớm: $(Get-Content $hubErr -Raw)" }
        Write-Host "Hub pid $($hub.Id) trên cổng $Port; idle $IdleSec s, tải $LoadSec s ($Sessions phiên, $Rate/s), nghỉ $RestSec s"
        $null = Add-Sample $hub.Id @('*') 'idle'   # mẫu gốc để tính CPU; không ghi vào CSV
        $script:Rows.Clear()

        Wait-Sampling $hub.Id @('*') 'idle' $IdleSec

        $replayOut = Join-Path $work 'replay.out'
        $replay = Start-Process -FilePath $replayExe -ArgumentList @($fix, '--url', "http://127.0.0.1:$Port", '--sessions', $Sessions, '--rate', $Rate, '--duration', $LoadSec, '--permission-mode', 'skip') `
            -PassThru -WindowStyle Hidden -RedirectStandardOutput $replayOut -RedirectStandardError (Join-Path $work 'replay.err')
        $started += $replay
        $loadEnd = (Get-Date).AddSeconds($LoadSec + 15)
        while (-not $replay.HasExited -and (Get-Date) -lt $loadEnd) {
            Start-Sleep -Seconds $IntervalSec
            $r = Add-Sample $hub.Id @('*') 'load'
            Write-Host ("[{0}] load   rss={1:N1} MB cpu={2}%" -f $r.ts.Substring(11, 8), ($r.rss_kb / 1024.0), $r.cpu_pct)
        }
        if (-not $replay.HasExited) { $replay.Kill() }
        $p99 = 0.0
        $text = if (Test-Path $replayOut) { Get-Content $replayOut -Raw } else { '' }
        if ($text -match '/hooks/event: \d+ yêu cầu, (\d+) lỗi, p50 [\d.,]+ ms, p95 [\d.,]+ ms, p99 ([\d.,]+) ms') {
            $p99 = [double]::Parse(($Matches[2] -replace ',', '.'), [System.Globalization.CultureInfo]::InvariantCulture)
        }
        Write-Host $text

        Wait-Sampling $hub.Id @('*') 'rest' $RestSec
        $csv = Join-Path $OutDir "hub-$stamp.csv"
        Save-Csv $csv
        $line = Get-HubSummary @(Read-Samples $csv) $p99 $commit
    }
    else {
        $appExe = Resolve-Bin $AppBin 'crates/app/src-tauri/target/release/arcturus-app.exe' {
            pnpm -C (Join-Path $root 'crates/app') tauri build --no-bundle
        }
        $appOut = Join-Path $work 'app.out'
        $appErr = Join-Path $work 'app.err'
        $delay = $DelaySec
        $env:ARCTURUS_HUB_BIN = $hubExe
        $env:ARCTURUS_HUB_ADDR = "127.0.0.1:$Port"
        $env:ARCTURUS_UI_TOKEN_FILE = Join-Path $work 'ui-token'
        $env:ARCTURUS_MEASURE_POPUP = "$Popups"
        $env:ARCTURUS_MEASURE_DELAY_SEC = "$delay"
        $env:ARCTURUS_MEASURE_HOLD_SEC = "$HoldSec"
        $env:ARCTURUS_MEASURE_HOVER_MS = "$HoverMs"
        $app = Start-Process -FilePath $appExe -PassThru -RedirectStandardOutput $appOut -RedirectStandardError $appErr
        $started += $app
        Write-Host "App pid $($app.Id); tray $delay s, rồi $Popups lần mở/đóng popup, giữ mở $HoldSec s"
        Start-Sleep -Seconds 3
        $null = Add-Sample $app.Id @('hub', 'conhost') 'tray'
        $script:Rows.Clear()

        $events = @()
        $deadline = (Get-Date).AddSeconds($delay + $Popups * 6 + $HoldSec + 90)
        $doneAt = $null
        while ((Get-Date) -lt $deadline) {
            Start-Sleep -Seconds $IntervalSec
            # Đọc sự kiện SAU khi ngủ, ngay trước khi lấy mẫu: nhãn pha phải khớp thời điểm mẫu.
            $events = @()
            if (Test-Path $appErr) {
                $events = @(Get-Content $appErr -ErrorAction SilentlyContinue | Where-Object { $_ -match '^\[measure\] ' })
            }
            $holdStart = $events | Where-Object { $_ -match 'event=hold_start' } | Select-Object -First 1
            $holdEnd = $events | Where-Object { $_ -match 'event=hold_end' } | Select-Object -First 1
            $cycleStart = $events | Where-Object { $_ -match 'event=cycle_start' } | Select-Object -First 1
            $phase = 'tray'
            if ($holdEnd) { $phase = 'after_close' }
            elseif ($holdStart) { $phase = 'popup_open' }
            elseif ($cycleStart) { $phase = 'popup_cycle' }
            $r = Add-Sample $app.Id @('hub', 'conhost') $phase
            Write-Host ("[{0}] {1,-11} rss={2:N1} MB cpu={3}%" -f $r.ts.Substring(11, 8), $phase, ($r.rss_kb / 1024.0), $r.cpu_pct)
            if ($holdEnd -and -not $doneAt) { $doneAt = Get-Date }
            if ($doneAt -and ((Get-Date) - $doneAt).TotalSeconds -ge 30) { break }
        }
        $ready = @()
        foreach ($e in $events) {
            if ($e -match 'event=popup_ready ms=(\d+)') { $ready += [double]$Matches[1] }
        }
        $csv = Join-Path $OutDir "app-$stamp.csv"
        Save-Csv $csv
        $line = Get-AppSummary @(Read-Samples $csv) $ready $commit
        Copy-Item $appErr (Join-Path $OutDir "app-$stamp.log") -ErrorAction SilentlyContinue
    }
    Write-Host "CSV: $csv"
    Write-Output $line
    if ($line -match 'verdict=FAIL') { $script:Exit = 1 } else { $script:Exit = 0 }
}
finally {
    foreach ($p in $started) {
        try { if (-not $p.HasExited) { $p.Kill() } } catch { }
    }
    if ($null -ne $savedToken) { $env:ARCTURUS_TOKEN = $savedToken } else { Remove-Item Env:ARCTURUS_TOKEN -ErrorAction SilentlyContinue }
    foreach ($n in 'ARCTURUS_HUB_BIN', 'ARCTURUS_HUB_ADDR', 'ARCTURUS_UI_TOKEN_FILE', 'ARCTURUS_MEASURE_POPUP', 'ARCTURUS_MEASURE_DELAY_SEC', 'ARCTURUS_MEASURE_HOLD_SEC', 'ARCTURUS_MEASURE_HOVER_MS') {
        Remove-Item "Env:$n" -ErrorAction SilentlyContinue
    }
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
exit $script:Exit
