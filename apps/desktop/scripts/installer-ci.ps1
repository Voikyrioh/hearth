# Preuve de l'installateur NSIS sur le runner Windows jetable de la CI (HRT-21).
# NE JAMAIS lancer sur un poste de travail : l'installateur est execute en vrai (registre, dossier
# d'installation). Sortie : installer-evidence/ (captures, journal des controles, resultats),
# publiee en artefact par le job `desktop`. Echoue (exit 1) a la premiere attente non tenue.
#
# Partie 1, page d'accueil : la case est la, visible, sans recouvrement, dans le bon etat initial
#   (installation neuve : decochee ; entree activee : cochee ; entree desactivee dans le
#   Gestionnaire des taches : decochee), capturee en image ; parcours complet case cochee.
# Partie 2, comportements : installation silencieuse (a), reinstallation silencieuse et mise a jour
#   passive avec une valeur posee comme le ferait l'application (b), mise a jour passive sans valeur
#   (c), desinstallation (d).
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$installer = (Get-ChildItem (Join-Path $root 'target\release\bundle\nsis\*.exe') | Select-Object -First 1).FullName
$evidence = Join-Path $root 'installer-evidence'
New-Item -ItemType Directory -Force $evidence | Out-Null
$dir = Join-Path $env:RUNNER_TEMP 'hearth-installed'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$approvedKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
$results = New-Object System.Collections.Generic.List[string]

function Save-Results {
  $results | Set-Content (Join-Path $evidence 'results.txt')
  if ($env:GITHUB_STEP_SUMMARY) {
    ('### Installateur NSIS sur le runner' + "`n`n" + (($results | ForEach-Object { "- $_" }) -join "`n")) | Add-Content $env:GITHUB_STEP_SUMMARY
  }
}
function Note($text) { Write-Host $text; $results.Add($text) }
function Fail($text) { Note "ECHEC : $text"; Save-Results; exit 1 }
function Expect($condition, $text) { if ($condition) { Note "ok : $text" } else { Fail $text } }

trap { Note "exception : $_"; Save-Results; exit 1 }

Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public static class W {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, int m, IntPtr w, IntPtr l);
  [DllImport("user32.dll", SetLastError = true)] static extern IntPtr SendMessageTimeout(IntPtr h, int m, IntPtr w, IntPtr l, int flags, int ms, out IntPtr res);
  [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern IntPtr SendMessageTimeout(IntPtr h, int m, IntPtr w, StringBuilder l, int flags, int ms, out IntPtr res);
  // Jamais de SendMessage nu : une fenetre de l'installateur occupee ou bloquee par une boite de dialogue ferait pendre la CI.
  public static int Ask(IntPtr h, int m) { IntPtr r; return SendMessageTimeout(h, m, IntPtr.Zero, IntPtr.Zero, 2, 5000, out r) == IntPtr.Zero ? -1 : (int)r; }
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  public static List<IntPtr> Children(IntPtr p) { var l = new List<IntPtr>(); EnumChildWindows(p, (h, x) => { l.Add(h); return true; }, IntPtr.Zero); return l; }
  public static string Text(IntPtr h) { var s = new StringBuilder(2048); IntPtr r; SendMessageTimeout(h, 0x000D, (IntPtr)2048, s, 2, 5000, out r); return s.ToString(); }
  public static string Cls(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
}
"@
[void][W]::SetProcessDPIAware()

function Shot($path, $window) {
  $r = New-Object W+RECT
  if ($window -ne [IntPtr]::Zero -and [W]::GetWindowRect($window, [ref]$r)) {
    $m = 12
    $box = [System.Drawing.Rectangle]::FromLTRB([Math]::Max(0, $r.L - $m), [Math]::Max(0, $r.T - $m), $r.R + $m, $r.B + $m)
  } else { $box = [System.Windows.Forms.SystemInformation]::VirtualScreen }
  $bmp = New-Object System.Drawing.Bitmap $box.Width, $box.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($box.Location, [System.Drawing.Point]::Empty, $box.Size)
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
}

function Run-Value { (Get-Item $runKey).GetValue('Hearth', $null) }
function Clear-Entry {
  Remove-ItemProperty -Path $runKey -Name Hearth -ErrorAction SilentlyContinue
  Remove-ItemProperty -Path $approvedKey -Name Hearth -ErrorAction SilentlyContinue
}
function Set-Approved($enabled) {
  New-Item -Path $approvedKey -Force | Out-Null
  if ($enabled) { $bytes = [byte[]](2,0,0,0,0,0,0,0,0,0,0,0) } else { $bytes = [byte[]](3,0,0,0,1,2,3,4,5,6,7,8) }
  New-ItemProperty -Path $approvedKey -Name Hearth -PropertyType Binary -Value $bytes -Force | Out-Null
}

function Start-Gui {
  $p = Start-Process $installer -ArgumentList ('/D=' + $dir) -PassThru
  $deadline = (Get-Date).AddSeconds(90)
  do { Start-Sleep -Milliseconds 500; $p.Refresh() } until ($p.MainWindowHandle -ne [IntPtr]::Zero -or (Get-Date) -gt $deadline -or $p.HasExited)
  if ($p.HasExited -or $p.MainWindowHandle -eq [IntPtr]::Zero) { Fail "la fenetre de l'installateur n'apparait pas (sortie : $($p.HasExited))" }
  return $p
}

function Find-Box($main) {
  $deadline = (Get-Date).AddSeconds(30)
  do {
    foreach ($h in [W]::Children($main)) {
      if ([W]::Cls($h) -eq 'Button' -and [W]::Text($h) -like 'Lancer Hearth*') { return $h }
    }
    Start-Sleep -Milliseconds 500
  } until ((Get-Date) -gt $deadline)
  return [IntPtr]::Zero
}

function Controls($main, $path) {
  $rows = @()
  foreach ($h in [W]::Children($main)) {
    $r = New-Object W+RECT; [void][W]::GetWindowRect($h, [ref]$r)
    $rows += [pscustomobject]@{ handle = $h.ToInt64(); class = [W]::Cls($h); text = [W]::Text($h); visible = [W]::IsWindowVisible($h); left = $r.L; top = $r.T; right = $r.R; bottom = $r.B }
  }
  $rows | ConvertTo-Json | Set-Content $path
  return $rows
}

function Welcome-Scenario($name, $setup, $expectedChecked) {
  Clear-Entry; & $setup
  $p = Start-Gui
  try {
    $main = $p.MainWindowHandle
    $box = Find-Box $main
    if ($box -eq [IntPtr]::Zero) { Shot (Join-Path $evidence "$name-no-box.png") $main; [void](Controls $main (Join-Path $evidence "$name-controls.json")); Fail "$name : la case n'existe pas sur la page d'accueil" }
    Start-Sleep -Seconds 2
    [void][W]::SetForegroundWindow($main)
    Shot (Join-Path $evidence "$name.png") $main
    $rows = Controls $main (Join-Path $evidence "$name-controls.json")
    $page = @($rows | Where-Object { $_.visible -and ($_.text -match 'Lancer Hearth|Hearth s.ouvre|Cet assistant') })
    Expect ($page.Count -eq 3) "$name : texte d'accueil, case et aide sont visibles ($($page.Count) sur 3)"
    $overlap = 0
    for ($i = 0; $i -lt $page.Count; $i++) { for ($j = $i + 1; $j -lt $page.Count; $j++) {
      $a = $page[$i]; $b = $page[$j]
      if ($a.left -lt $b.right -and $b.left -lt $a.right -and $a.top -lt $b.bottom -and $b.top -lt $a.bottom) { $overlap++ }
    } }
    Expect ($overlap -eq 0) "$name : aucun recouvrement entre les trois controles"
    $state = [W]::Ask($box, 0xF0)
    $got = if ($state -eq 1) { 'cochee' } else { 'decochee' }
    $want = if ($expectedChecked) { 'cochee' } else { 'decochee' }
    Expect (($state -eq 1) -eq $expectedChecked) "$name : case $got (attendu : $want)"
  } finally { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }; Start-Sleep -Seconds 2 }
}

# Parcours complet : (de)coche la case si demande, clique Suivant jusqu'a la fin.
function Full-Flow($name, $toggle) {
  $p = Start-Gui
  $main = $p.MainWindowHandle
  $box = Find-Box $main
  if ($box -eq [IntPtr]::Zero) { Fail "$name : pas de case" }
  if ($toggle) { [void][W]::PostMessage($box, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero); Start-Sleep -Milliseconds 500 }
  Shot (Join-Path $evidence "$name-before-next.png") $main
  $deadline = (Get-Date).AddSeconds(240)
  $tick = 0
  while (-not $p.HasExited -and (Get-Date) -lt $deadline) {
    $next = [W]::GetDlgItem($main, 1)
    if ($next -ne [IntPtr]::Zero) { [void][W]::PostMessage($next, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) }
    Start-Sleep -Seconds 2
    $tick++
    if ($tick % 5 -eq 0 -and $tick -le 40) { Shot (Join-Path $evidence ("{0}-etape-{1:00}.png" -f $name, $tick)) $main }
  }
  if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force; Fail "$name : l'installateur ne se termine pas" }
  Get-Process | Where-Object { $_.Path -and $_.Path -like "$dir*" } | Stop-Process -Force -ErrorAction SilentlyContinue
}

function Silent($arguments) {
  $p = Start-Process $installer -ArgumentList ($arguments + @('/D=' + $dir)) -PassThru
  if (-not $p.WaitForExit(240000)) { $p.Kill(); Fail "installateur $arguments : delai depasse" }
  Expect ($p.ExitCode -eq 0) "installateur $($arguments -join ' ') : code de sortie $($p.ExitCode)"
}

Note "installateur : $installer"
Expect ($null -eq (Run-Value)) 'runner vierge : aucune valeur Hearth sous Run au depart'

# ---- Partie 1 : la page d'accueil
Welcome-Scenario 'accueil-neuf' { } $false
Welcome-Scenario 'accueil-entree-activee' {
  New-ItemProperty -Path $runKey -Name Hearth -Value 'C:\sentinel\Hearth.exe --minimized' -Force | Out-Null; Set-Approved $true
} $true
Welcome-Scenario 'accueil-entree-desactivee-gestionnaire' {
  New-ItemProperty -Path $runKey -Name Hearth -Value 'C:\sentinel\Hearth.exe --minimized' -Force | Out-Null; Set-Approved $false
} $false
Clear-Entry

# Parcours complet, case cochee : l'entree est ecrite comme le greffon (sans guillemets, activee).
Full-Flow 'parcours-coche' $true
$value = Run-Value
Expect ($null -ne $value) 'parcours case cochee : valeur Hearth presente sous Run'
Expect (($value -match '\.exe --minimized$') -and ($value -notmatch '"')) "parcours case cochee : valeur sans guillemets, finit par --minimized ($value)"
$approved = (Get-Item $approvedKey).GetValue('Hearth', $null)
Expect (($approved -join ',') -eq '2,0,0,0,0,0,0,0,0,0,0,0') 'parcours case cochee : active dans le Gestionnaire des taches'

# Reinstallation a la main : la case arrive cochee (entree activee) ; la decocher retire l'entree.
Full-Flow 'parcours-decoche-reinstallation' $true
Expect ($null -eq (Run-Value)) 'reinstallation a la main, case decochee : valeur retiree'

# ---- Partie 2 : comportements sans interface
$uninstaller = Join-Path $dir 'uninstall.exe'
if (Test-Path $uninstaller) { Start-Process $uninstaller -ArgumentList '/S', "_?=$dir" -Wait }
Clear-Entry
if (Test-Path $dir) { Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue }

Silent @('/S')
Expect ($null -eq (Run-Value)) '(a) installation silencieuse sur runner vierge : aucune valeur Hearth sous Run'
Expect (@(Get-ChildItem $dir -Filter *.exe).Count -ge 1) '(a) les fichiers sont installes'

$sentinel = 'C:\sentinel\Hearth.exe --minimized'
New-ItemProperty -Path $runKey -Name Hearth -Value $sentinel -Force | Out-Null
Silent @('/S')
Expect ((Run-Value) -ceq $sentinel) '(b) valeur posee a la main puis reinstallation silencieuse : intacte'
Silent @('/UPDATE', '/P')
Expect ((Run-Value) -ceq $sentinel) '(b) valeur posee a la main puis mise a jour passive (/UPDATE /P) : intacte'

Remove-ItemProperty -Path $runKey -Name Hearth
Silent @('/UPDATE', '/P')
Expect ($null -eq (Run-Value)) '(c) valeur absente puis mise a jour passive : toujours absente'
Silent @('/S')
Expect ($null -eq (Run-Value)) '(c) valeur absente puis reinstallation silencieuse : toujours absente'

New-ItemProperty -Path $runKey -Name Hearth -Value $sentinel -Force | Out-Null
$u = Start-Process $uninstaller -ArgumentList '/S', "_?=$dir" -PassThru
[void]$u.WaitForExit(240000)
Expect ($null -eq (Run-Value)) '(d) desinstallation silencieuse : valeur retiree'

Clear-Entry
Save-Results
