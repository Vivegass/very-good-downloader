param([string]$Exe, [string]$Root, [string]$Out)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public static IntPtr Biggest(uint pid) {
    IntPtr best = IntPtr.Zero; long area = 0;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && IsWindowVisible(h)) {
        RECT r; GetWindowRect(h, out r);
        long a = (long)(r.R - r.L) * (r.B - r.T);
        if (a > area) { area = a; best = h; }
      }
      return true;
    }, IntPtr.Zero);
    return best;
  }
}
'@
[W]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force $Out | Out-Null
foreach ($d in Get-ChildItem $Root -Directory) {
  $page = (Get-Content (Join-Path $d.FullName 'page.txt')).Trim()
  $env:APPDATA = $d.FullName
  $p = if ($page -eq 'settings') { Start-Process $Exe -ArgumentList '--page=settings' -PassThru } else { Start-Process $Exe -PassThru }
  $h = [IntPtr]::Zero
  for ($k = 0; $k -lt 40; $k++) {
    Start-Sleep -Milliseconds 250
    $h = [W]::Biggest([uint32]$p.Id)
    if ($h -ne [IntPtr]::Zero) { $r0 = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$r0) | Out-Null; if (($r0.R - $r0.L) -gt 300) { break } }
  }
  [W]::ShowWindow($h, 1) | Out-Null
  [W]::SetForegroundWindow($h) | Out-Null
  Start-Sleep -Milliseconds 2200
  # только содержимое окна, без рамки и заголовка
  $c = New-Object W+RECT; [W]::GetClientRect($h, [ref]$c) | Out-Null
  $pt = New-Object W+POINT; [W]::ClientToScreen($h, [ref]$pt) | Out-Null
  $bmp = New-Object Drawing.Bitmap $c.R, $c.B
  $g = [Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($pt.X, $pt.Y, 0, 0, $bmp.Size)
  $bmp.Save((Join-Path $Out "$($d.Name).png"), [Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Stop-Process -Id $p.Id -Force
  Start-Sleep -Milliseconds 400
}
