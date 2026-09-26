<#
.SYNOPSIS
  VERIFY for the console's secret handling (DECISIONS.md, "Console private-key
  output"): scans the REAL `bitcoin-cli help <method>` text of every method of
  the installed Bitcoin Core for (1) private-key language anywhere, and (2)
  positional parameters that look like a secret (passphrase / password / seed /
  private key), against a throwaway regtest node. Read-only; the node lives in a
  temp folder that is removed afterwards.

  Windows PowerShell:  powershell -NoProfile -File scan_help_for_secrets.ps1 -BinDir <folder holding bitcoind.exe and bitcoin-cli.exe>
#>
param(
    [Parameter(Mandatory = $true)][string]$BinDir,
    [int]$RpcPort = 28443,
    # Only print the sorted list of every method name (the source of
    # KNOWN_BITCOIN_RPC_METHODS in crates/nk-core/src/console_safety.rs).
    [switch]$ListOnly
)
$cli = Join-Path $BinDir 'bitcoin-cli.exe'; $bd = Join-Path $BinDir 'bitcoind.exe'
if (Get-Process bitcoind, ord -ErrorAction SilentlyContinue) { "ABORT: bitcoind/ord already running"; return }
$work = Join-Path $env:TEMP ("nk-help-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory $work | Out-Null
$common = @("-regtest", "-datadir=$work", "-rpcport=$RpcPort")
$p = Start-Process $bd -ArgumentList ($common + @("-port=$($RpcPort + 1)", "-server=1", "-listen=0", "-dnsseed=0", "-fixedseeds=0")) -PassThru -WindowStyle Hidden
try {
    $ok = $false
    for ($i = 0; $i -lt 60 -and -not $ok; $i++) { Start-Sleep -Milliseconds 500; & $cli @common getblockcount 2>$null | Out-Null; $ok = ($LASTEXITCODE -eq 0) }
    "node ready: $ok"
    "version: $((& $cli @common getnetworkinfo | ConvertFrom-Json).subversion)"
    $methods = (& $cli @common help) | Where-Object { $_ -match '^[a-z]' } | ForEach-Object { ($_ -split ' ')[0] } | Sort-Object -Unique
    "methods: $($methods.Count)"
    if ($ListOnly) {
        "=== all methods ==="; $methods
        # `help` does not list the "hidden" RPCs (regtest tooling, test hooks),
        # but `help <name>` answers for one that exists. Candidates only; each
        # is checked against the node, never assumed.
        $candidates = 'addconnection', 'addpeeraddress', 'echo', 'echoipc', 'echojson', 'generate', 'generateblock',
            'generatetoaddress', 'generatetodescriptor', 'getorphantxs', 'getrawaddrman', 'invalidateblock',
            'mockscheduler', 'reconsiderblock', 'sendmsgtopeer', 'setmocktime', 'syncwithvalidationinterfacequeue',
            'waitforblock', 'estimaterawfee', 'getaddressbylabel', 'gettxoutsetinfo', 'setnetworkactive'
        "=== hidden methods that exist (help <name> answers) ==="
        foreach ($c in ($candidates | Sort-Object -Unique)) {
            if ($methods -contains $c) { continue }
            $text = (& $cli @common help $c 2>&1) -join "`n"
            if ($LASTEXITCODE -eq 0 -and $text -notmatch 'unknown command') { $c }
        }
        return
    }

    "`n=== (1) methods whose help text mentions private-key language ==="
    foreach ($m in $methods) {
        $t = (& $cli @common help $m) -join "`n"
        $found = [regex]::Matches($t, '(?i)private key|private descriptor|xprv|tprv|\bseed\b|mnemonic|secret|\bWIF\b|"private"|privkey') |
            ForEach-Object { $_.Value.ToLower() } | Sort-Object -Unique
        if ($found) { "{0,-28} {1}" -f $m, ($found -join ', ') }
    }

    "`n=== (2) positional parameters that look like a secret (the Arguments section) ==="
    foreach ($m in $methods) {
        $inArgs = $false; $hits = @()
        foreach ($line in (& $cli @common help $m)) {
            if ($line -match '^Arguments:') { $inArgs = $true; continue }
            if ($inArgs -and $line -match '^(Result|Examples|Named Arguments):') { $inArgs = $false }
            if ($inArgs -and $line -match '^\d+\.\s' -and $line -match '(?i)passphrase|password|mnemonic|seed|priv') { $hits += $line.Trim() }
        }
        if ($hits) { "{0}:" -f $m; $hits | ForEach-Object { "    $_" } }
    }

    "`n=== signatures that matter ==="
    foreach ($m in 'gethdkeys', 'listdescriptors', 'createwallet', 'migratewallet') {
        "--- help $m ---"
        (& $cli @common help $m) | Select-Object -First 16
    }
}
finally {
    & $cli @common stop 2>$null | Out-Null
    Start-Sleep -Seconds 3
    if (-not $p.HasExited) { $p.Kill() }
    if ($work.StartsWith($env:TEMP) -and $work.Contains('nk-help-')) { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
}
