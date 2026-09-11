# Global key injector: AltV | Down | Enter | Escape etc.
param([string]$seq)
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class KI {
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint fl, UIntPtr ex);
  public static void Tap(byte vk) {
    keybd_event(vk,0,0,UIntPtr.Zero);
    System.Threading.Thread.Sleep(30);
    keybd_event(vk,0,2,UIntPtr.Zero);
  }
  public static void AltV() {
    keybd_event(0x12,0,0,UIntPtr.Zero);
    Tap(0x56);
    keybd_event(0x12,0,2,UIntPtr.Zero);
  }
}
"@
switch ($seq) {
  'AltV' { [KI]::AltV() }
  'Down' { [KI]::Tap(0x28) }
  'Up'   { [KI]::Tap(0x26) }
  'Enter'{ [KI]::Tap(0x0D) }
}
