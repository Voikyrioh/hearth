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
# Chemin AVEC une espace (comme un profil Windows « Jean Dupont », HRT-29) : toutes les etapes y passent.
$dir = Join-Path $env:RUNNER_TEMP 'hearth installed'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$approvedKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
$results = New-Object System.Collections.Generic.List[string]

function Save-Results {
  $results | Set-Content (Join-Path $evidence "results.txt")
  if ($env:GITHUB_STEP_SUMMARY) {
    ("### Installateur NSIS sur le runner" + "`n`n" + (($results | ForEach-Object { "- $_" }) -join "`n")) | Add-Content $env:GITHUB_STEP_SUMMARY
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
  public delegate bool TopProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(TopProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public static List<IntPtr> TopLevels(uint pid) { var l = new List<IntPtr>(); EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid) l.Add(h); return true; }, IntPtr.Zero); return l; }
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
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

# Pixels sombres dans un rectangle de l'ECRAN : une case dessinee en a (bordure, coche, texte),
# une zone recouverte ou vide n'en a aucun. IsWindowVisible ne voit pas un recouvrement, les pixels si.
function Ink($left, $top, $width, $height) {
  $bmp = New-Object System.Drawing.Bitmap $width, $height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($left, $top, 0, 0, (New-Object System.Drawing.Size $width, $height))
  $dark = 0
  for ($x = 0; $x -lt $width; $x++) { for ($y = 0; $y -lt $height; $y++) {
    $c = $bmp.GetPixel($x, $y); if (($c.R + $c.G + $c.B) -lt 450) { $dark++ }
  } }
  $g.Dispose(); $bmp.Dispose()
  return $dark
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
    # Aucun AUTRE controle visible (hors dialogue de page) ne recouvre la case.
    $b = $rows | Where-Object { $_.handle -eq $box.ToInt64() }
    $covering = @($rows | Where-Object {
      $_.visible -and $_.handle -ne $box.ToInt64() -and $_.class -ne '#32770' -and ($_.right - $_.left) -gt 0 -and
      $_.left -lt $b.right -and $b.left -lt $_.right -and $_.top -lt $b.bottom -and $b.top -lt $_.bottom })
    Expect ($covering.Count -eq 0) "$name : aucun controle visible ne recouvre la case ($(($covering | ForEach-Object { $_.class + ':' + $_.text }) -join ', '))"
    # Et les pixels le confirment : le carre de la case et son texte sont dessines.
    $ink = Ink $b.left $b.top ($b.right - $b.left) ($b.bottom - $b.top)
    Expect ($ink -ge 20) "$name : la case est dessinee a l'ecran ($ink pixels sombres)"
    $state = [W]::Ask($box, 0xF0)
    $got = if ($state -eq 1) { 'cochee' } else { 'decochee' }
    $want = if ($expectedChecked) { 'cochee' } else { 'decochee' }
    Expect (($state -eq 1) -eq $expectedChecked) "$name : case $got (attendu : $want)"
  } finally { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }; Start-Sleep -Seconds 2 }
}

# Parcours complet : (de)coche la case si demande, passe toutes les pages, puis ferme par $close
# ('mouse' : vrai clic de souris sur « Fermer » ; 'command' : WM_COMMAND IDOK a la fenetre principale, ce que
# produit un clic. Constate (CI du 2026-10-07, aussi sur l'installateur SANS nos crochets) : un BM_CLICK poste et
# un WM_CLOSE ne ferment PAS la page de fin du modele de Tauri ; c'est une limite de ces messages, pas de notre
# script NSIS. Un installateur qui ne sort pas fait ECHOUER le job.)
# $shortcut : laisse (ou non) cochee « Creer un raccourci sur le bureau ». Renvoie 'sortie' ou 'bloque'.
$script:stuck = New-Object System.Collections.Generic.List[string]
function Full-Flow($name, $toggle, $close, $shortcut) {
  $p = Start-Gui
  $main = $p.MainWindowHandle
  $box = Find-Box $main
  if ($box -eq [IntPtr]::Zero) { Fail "$name : pas de case" }
  if ($toggle) { [void][W]::PostMessage($box, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero); Start-Sleep -Milliseconds 500 }
  Shot (Join-Path $evidence "$name-accueil.png") $main
  $deadline = (Get-Date).AddSeconds(240)
  $finishSince = $null
  $tick = 0
  while (-not $p.HasExited -and (Get-Date) -lt $deadline) {
    $onFinish = $false
    foreach ($h in [W]::Children($main)) {
      if ([W]::Cls($h) -ne 'Button') { continue }
      $t = [W]::Text($h)
      if ($t -eq 'Lancer Hearth') {
        $onFinish = $true
        if ([W]::Ask($h, 0xF0) -eq 1) { [void][W]::PostMessage($h, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) }
      }
      if ((-not $shortcut) -and ($t -like 'Cr*er un raccourci*') -and ([W]::Ask($h, 0xF0) -eq 1)) { [void][W]::PostMessage($h, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) }
    }
    $next = [W]::GetDlgItem($main, 1)
    if ($onFinish) {
      if ($null -eq $finishSince) {
        $finishSince = Get-Date; Start-Sleep -Seconds 3
        Shot (Join-Path $evidence "$name-fin.png") $main
        Shot (Join-Path $evidence "$name-fin-ecran.png") ([IntPtr]::Zero)
      }
      switch ($close) {
        'command' { [void][W]::PostMessage($main, 0x111, [IntPtr]1, $next) }
        'close' { [void][W]::PostMessage($main, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) }
        'mouse' {
          $r = New-Object W+RECT
          if ([W]::GetWindowRect($next, [ref]$r)) {
            [void][W]::SetForegroundWindow($main)
            [void][W]::SetCursorPos([int](($r.L + $r.R) / 2), [int](($r.T + $r.B) / 2))
            [W]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero); [W]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
          }
        }
        default { [void][W]::PostMessage($next, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) }
      }
      if (((Get-Date) - $finishSince).TotalSeconds -gt 25) { break }
    } elseif ($next -ne [IntPtr]::Zero) { [void][W]::PostMessage($next, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) }
    Start-Sleep -Seconds 2
    $tick++
    if (($tick % 6 -eq 0) -and ($tick -le 36) -and (-not $onFinish)) { Shot (Join-Path $evidence ("{0}-etape-{1:00}.png" -f $name, $tick)) $main }
  }
  if ($p.HasExited) {
    $secs = if ($finishSince) { [int]((Get-Date) - $finishSince).TotalSeconds } else { -1 }
    Note "$name ($close, raccourci=$shortcut) : l'installateur se ferme ($secs s apres la page de fin, code $($p.ExitCode))"
    $result = 'sortie'
  } else {
    $tops = [W]::TopLevels([uint32]$p.Id) | ForEach-Object { "{0}:'{1}':{2}" -f [W]::Cls($_), [W]::Text($_), [W]::IsWindowVisible($_) }
    Shot (Join-Path $evidence "$name-bloque.png") $main
    Shot (Join-Path $evidence "$name-bloque-ecran.png") ([IntPtr]::Zero)
    Note "BLOQUE $name ($close, raccourci=$shortcut) : l'installateur ne se ferme pas. Fenetres du processus : $($tops -join ' | ')"
    $script:stuck.Add("$name ($close, raccourci=$shortcut)")
    Stop-Process -Id $p.Id -Force
    $result = 'bloque'
  }
  Start-Sleep -Seconds 2
  Get-Process | Where-Object { $_.Path -and $_.Path -like "$dir*" } | Stop-Process -Force -ErrorAction SilentlyContinue
  return $result
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

# Parcours complet, case cochee : l'entree est ecrite comme l'application (chemin entre guillemets, activee).
[void](Full-Flow 'parcours-coche' $true 'mouse' $true)
$value = Run-Value
Expect ($null -ne $value) 'parcours case cochee : valeur Hearth presente sous Run'
$installedExe = (Get-ChildItem $dir -Filter *.exe | Where-Object { $_.Name -notlike 'uninstall*' } | Select-Object -First 1).FullName
Expect ($dir -match ' ') "parcours case cochee : le dossier d'installation contient une espace ($dir)"
Expect ($value -ceq ('"' + $installedExe + '" --minimized')) "parcours case cochee : valeur exacte entre guillemets, chemin avec espace ($value)"
Expect (@((Get-Item $runKey).GetValueNames() | Where-Object { $_ -like 'Hearth*' }).Count -eq 1) 'parcours case cochee : une seule valeur Hearth sous Run (jamais de seconde entree)'
$approved = (Get-Item $approvedKey).GetValue('Hearth', $null)
Expect (($approved -join ',') -eq '2,0,0,0,0,0,0,0,0,0,0,0') 'parcours case cochee : active dans le Gestionnaire des taches'

# Reinstallation a la main : la case arrive cochee (entree activee) ; la decocher retire l'entree.
[void](Full-Flow 'reinstallation-decoche' $true 'command' $false)
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

# (e) Chemin d'installation avec une espace : la valeur ENTRE GUILLEMETS (celle qu'ecrivent l'application
# et l'installateur) lance-t-elle la bonne application ? CreateProcess la decoupe sans ambiguite.
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Launch {
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)] public struct SI { public int cb; public string r, d, t; public int x, y, w, h, cx, cy, fa, fl; public short sw, cb2; public IntPtr lr, i, o, e; }
  [StructLayout(LayoutKind.Sequential)] public struct PI { public IntPtr hp, ht; public int pid, tid; }
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] public static extern bool CreateProcessW(string app, string cmd, IntPtr pa, IntPtr ta, bool inh, int flags, IntPtr env, string cwd, ref SI si, out PI pi);
}
"@
$spaced = Join-Path $env:RUNNER_TEMP 'Jean Dupont\AppData\Local\Hearth'
New-Item -ItemType Directory -Force $spaced | Out-Null
$sp = Start-Process $installer -ArgumentList '/S', ('/D=' + $spaced) -PassThru
[void]$sp.WaitForExit(240000)
$exe = (Get-ChildItem $spaced -Filter *.exe | Where-Object { $_.Name -notlike 'uninstall*' } | Select-Object -First 1).FullName
Expect ($null -ne $exe) "(e) installation dans un chemin avec espace : $exe"
$command = """$exe"" --minimized"
$si = New-Object Launch+SI; $si.cb = [Runtime.InteropServices.Marshal]::SizeOf($si)
$pi = New-Object Launch+PI
# [NullString]::Value : PowerShell convertirait $null en chaine vide (ERREUR 123 constatee sur la CI).
$ok = [Launch]::CreateProcessW([NullString]::Value, $command, [IntPtr]::Zero, [IntPtr]::Zero, $false, 0, [IntPtr]::Zero, [NullString]::Value, [ref]$si, [ref]$pi)
$err = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
Start-Sleep -Seconds 3
$started = if ($ok) { Get-Process -Id $pi.pid -ErrorAction SilentlyContinue } else { $null }
Note "(e) valeur entre guillemets avec espace, CreateProcess(NULL, ""$command"") : ok=$ok erreur=$err processus=$($started.Path)"
Expect ($ok -and ($started.Path -eq $exe)) '(e) la valeur entre guillemets lance bien Hearth avec une espace dans le chemin (CreateProcess)'
if ($started) { Stop-Process -Id $pi.pid -Force }
$su = Start-Process (Join-Path $spaced 'uninstall.exe') -ArgumentList '/S', "_?=$spaced" -PassThru
[void]$su.WaitForExit(120000)

Clear-Entry
if ($script:stuck.Count -gt 0) { Fail "l'installateur ne se ferme pas : $($script:stuck -join ' ; ')" }
Save-Results
